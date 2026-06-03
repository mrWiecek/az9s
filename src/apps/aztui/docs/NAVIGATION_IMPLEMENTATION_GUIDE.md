# Navigation Implementation Guide

## Overview

This guide provides concrete implementation steps for achieving k9s-like navigation and interaction fidelity in aztui. It focuses on **actionable tasks** for the next iterations.

---

## Iteration 4: View Transitions & Breadcrumbs

### Goal
Enable seamless navigation between subscription → resource groups → resources with clear breadcrumb trail and state preservation.

### Tasks

#### 1. Implement Breadcrumb Component

**File**: `crates/aztui-tui/src/ui/breadcrumb.rs` (new)

```rust
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub struct Breadcrumb {
    segments: Vec<String>,
}

impl Breadcrumb {
    pub fn new() -> Self {
        Self {
            segments: vec!["aztui".to_string()],
        }
    }

    pub fn push(&mut self, segment: String) {
        self.segments.push(segment);
    }

    pub fn pop(&mut self) -> Option<String> {
        if self.segments.len() > 1 {
            self.segments.pop()
        } else {
            None
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let spans: Vec<Span> = self
            .segments
            .iter()
            .enumerate()
            .flat_map(|(i, segment)| {
                let mut result = vec![];
                if i > 0 {
                    result.push(Span::styled(
                        " > ",
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                result.push(Span::styled(
                    segment,
                    if i == self.segments.len() - 1 {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                ));
                result
            })
            .collect();

        let breadcrumb = Paragraph::new(Line::from(spans));
        f.render_widget(breadcrumb, area);
    }
}
```

**Integration**:
- Add `breadcrumb: Breadcrumb` to `AppState`
- Update breadcrumb on view transitions
- Render in header area

#### 2. Enhance View Transition Logic

**File**: `crates/aztui-tui/src/app.rs`

```rust
async fn handle_subscription_select(&mut self) -> Result<bool> {
    if let Some(subscription) = self.state().selected_subscription().cloned() {
        info!("Selected subscription: {}", subscription.name);
        
        // Set loading state
        self.state_mut().set_loading(true);
        
        // Fetch resource groups
        match self.core_mut().fetch_resource_groups(&subscription.id).await {
            Ok(resource_groups) => {
                let state = self.state_mut();
                
                // Update state
                state.selected_subscription = Some(subscription.clone());
                state.resource_groups = resource_groups;
                
                // Push navigation context
                state.push_navigation(NavigationContext {
                    view: ViewMode::SubscriptionList,
                    subscription_id: Some(subscription.id.clone()),
                    resource_group: None,
                    resource_id: None,
                    metadata: HashMap::new(),
                });
                
                // Change view
                state.view = ViewMode::ResourceGroupList;
                
                // Update breadcrumb
                state.breadcrumb.push(subscription.name.clone());
                
                // Reset list state
                state.list_state.reset();
                
                // Clear loading
                state.set_loading(false);
            }
            Err(e) => {
                self.state_mut().set_error(Some(format!("Failed to fetch resource groups: {}", e)));
                self.state_mut().set_loading(false);
            }
        }
    }
    Ok(true)
}
```

#### 3. Implement Back Navigation

**File**: `crates/aztui-tui/src/app.rs`

```rust
async fn handle_back_navigation(&mut self) -> Result<bool> {
    if let Some(context) = self.state_mut().pop_navigation() {
        // Restore previous view
        self.state_mut().view = context.view;
        
        // Pop breadcrumb
        self.state_mut().breadcrumb.pop();
        
        // Reset list state
        self.state_mut().list_state.reset();
        
        // Optionally restore previous selection
        // (requires storing selection in NavigationContext)
        
        info!("Navigated back to {:?}", context.view);
    }
    Ok(true)
}
```

Update `handle_key` to call `handle_back_navigation` on `Esc`:

```rust
KeyCode::Esc => {
    let state = self.state_mut();
    if state.command_palette_open {
        state.command_palette_open = false;
    } else if state.error.is_some() {
        state.clear_error();
    } else {
        return self.handle_back_navigation().await;
    }
    Ok(true)
}
```

#### 4. Add Resource Group View Rendering

**File**: `crates/aztui-tui/src/ui/mod.rs`

```rust
fn draw_resource_group_list(f: &mut Frame, area: Rect, app: &TuiApp) -> Result<()> {
    let resource_groups = &app.state().resource_groups;
    
    if resource_groups.is_empty() {
        let content = Paragraph::new("No resource groups found.\\n\\nPress Esc to go back.")
            .block(Block::default().borders(Borders::ALL).title("Resource Groups"))
            .style(Style::default().fg(Color::Gray));
        f.render_widget(content, area);
        return Ok(());
    }

    // Create list items with multi-column layout
    let items: Vec<_> = resource_groups
        .iter()
        .map(|rg| {
            // Format: NAME | LOCATION | RESOURCE_COUNT | TAGS
            let name = format!("{:<30}", truncate(&rg.name, 30));
            let location = format!("{:<15}", rg.location);
            let count = format!("{:>3} resources", rg.resource_count.unwrap_or(0));
            let tags = format_tags(&rg.tags, 20);
            
            list_item_spans(vec![
                Span::styled(name, Style::default().fg(Color::Green)),
                Span::raw(" "),
                Span::styled(location, Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(count, Style::default().fg(Color::Cyan)),
                Span::raw(" "),
                Span::styled(tags, Style::default().fg(Color::Gray)),
            ])
        })
        .collect();

    // Render with SelectableList
    let list = SelectableList::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Resource Groups ({})", resource_groups.len()))
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        );

    let mut list_state = SelectableListState {
        selected: app.state().list_state.selected,
        offset: app.state().list_state.offset,
    };

    list.render(area, f.buffer_mut(), &mut list_state);

    Ok(())
}

// Helper functions
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn format_tags(tags: &HashMap<String, String>, max_len: usize) -> String {
    if tags.is_empty() {
        return String::new();
    }
    
    let tag_str: String = tags
        .iter()
        .map(|(k, v)| format!("{}:{}", k, v))
        .collect::<Vec<_>>()
        .join(", ");
    
    truncate(&tag_str, max_len)
}
```

#### 5. Add Column Headers

**File**: `crates/aztui-tui/src/ui/widgets.rs`

Add a `ColumnHeader` widget:

```rust
pub struct ColumnHeader<'a> {
    columns: Vec<(&'a str, u16)>, // (name, width)
}

impl<'a> ColumnHeader<'a> {
    pub fn new(columns: Vec<(&'a str, u16)>) -> Self {
        Self { columns }
    }
}

impl<'a> Widget for ColumnHeader<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut x = area.x;
        for (name, width) in self.columns {
            let header_span = Span::styled(
                format!("{:<width$}", name, width = width as usize),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            );
            
            for (i, ch) in header_span.content.chars().enumerate() {
                if x + i as u16 >= area.x + area.width {
                    break;
                }
                buf.get_mut(x + i as u16, area.y)
                    .set_char(ch)
                    .set_style(header_span.style);
            }
            
            x += width;
        }
    }
}
```

Use in list views:

```rust
// Render column headers
let header_area = Rect {
    x: list_area.x,
    y: list_area.y,
    width: list_area.width,
    height: 1,
};

let header = ColumnHeader::new(vec![
    ("NAME", 30),
    ("LOCATION", 15),
    ("RESOURCES", 12),
    ("TAGS", 20),
]);

header.render(header_area, f.buffer_mut());

// Adjust list area to start below header
let list_area = Rect {
    y: list_area.y + 1,
    height: list_area.height.saturating_sub(1),
    ..list_area
};
```

---

## Iteration 5: Status Bar & Context Display

### Goal
Add a comprehensive status bar with connection status, current context, and last refresh time.

### Tasks

#### 1. Create Status Bar Component

**File**: `crates/aztui-tui/src/ui/statusbar.rs` (new)

```rust
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use chrono::{DateTime, Utc};

pub struct StatusBar {
    connected: bool,
    subscription: Option<String>,
    resource_group: Option<String>,
    item_count: usize,
    last_refresh: Option<DateTime<Utc>>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            connected: false,
            subscription: None,
            resource_group: None,
            item_count: 0,
            last_refresh: None,
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let mut spans = vec![];

        // Connection status
        let (status_symbol, status_color) = if self.connected {
            ("●", Color::Green)
        } else {
            ("●", Color::Red)
        };
        spans.push(Span::styled(status_symbol, Style::default().fg(status_color)));
        spans.push(Span::raw(" "));
        spans.push(Span::raw(if self.connected { "Connected" } else { "Disconnected" }));

        // Subscription
        if let Some(sub) = &self.subscription {
            spans.push(Span::raw(" | Sub: "));
            spans.push(Span::styled(sub, Style::default().fg(Color::Cyan)));
        }

        // Resource group
        if let Some(rg) = &self.resource_group {
            spans.push(Span::raw(" | RG: "));
            spans.push(Span::styled(rg, Style::default().fg(Color::Yellow)));
        }

        // Item count
        if self.item_count > 0 {
            spans.push(Span::raw(format!(" | {} items", self.item_count)));
        }

        // Last refresh
        if let Some(last_refresh) = self.last_refresh {
            let elapsed = Utc::now().signed_duration_since(last_refresh);
            let time_str = if elapsed.num_seconds() < 60 {
                format!("{}s ago", elapsed.num_seconds())
            } else if elapsed.num_minutes() < 60 {
                format!("{}m ago", elapsed.num_minutes())
            } else {
                format!("{}h ago", elapsed.num_hours())
            };
            spans.push(Span::raw(" | ⟳ "));
            spans.push(Span::styled(time_str, Style::default().fg(Color::Gray)));
        }

        let status_bar = Paragraph::new(Line::from(spans));
        f.render_widget(status_bar, area);
    }
}
```

#### 2. Integrate Status Bar

**File**: `crates/aztui-core/src/state.rs`

Add fields to `AppState`:

```rust
pub struct AppState {
    // ... existing fields ...
    pub status_bar: StatusBar,
    pub last_refresh: Option<DateTime<Utc>>,
}
```

**File**: `crates/aztui-tui/src/ui/mod.rs`

Update layout to include status bar:

```rust
pub fn draw(f: &mut Frame, app: &TuiApp) -> Result<()> {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header with breadcrumb
            Constraint::Min(0),     // Main content
            Constraint::Length(1),  // Status bar
            Constraint::Length(3),  // Footer with keybindings
        ])
        .split(f.size());

    draw_header(f, chunks[0], app);
    draw_content(f, chunks[1], app)?;
    draw_status_bar(f, chunks[2], app);
    draw_footer(f, chunks[3], app);

    // ... error overlay ...

    Ok(())
}

fn draw_status_bar(f: &mut Frame, area: Rect, app: &TuiApp) {
    let mut status_bar = StatusBar::new();
    status_bar.connected = true; // TODO: Check actual connection
    
    if let Some(sub) = &app.state().selected_subscription {
        status_bar.subscription = Some(truncate(&sub.name, 20));
    }
    
    status_bar.item_count = app.state().current_list_len();
    status_bar.last_refresh = app.state().last_refresh;
    
    status_bar.render(f, area);
}
```

---

## Iteration 6: Enhanced Footer with Context-Aware Hints

### Goal
Make the footer dynamically show relevant keybindings based on current view and state.

### Tasks

#### 1. Create Context-Aware Keybinding System

**File**: `crates/aztui-tui/src/ui/keybindings.rs` (new)

```rust
use aztui_core::state::ViewMode;

pub struct Keybinding {
    pub key: &'static str,
    pub description: &'static str,
}

pub fn get_keybindings(view: ViewMode, has_selection: bool) -> Vec<Keybinding> {
    let mut bindings = vec![
        Keybinding { key: "q", description: "Quit" },
        Keybinding { key: "?", description: "Help" },
    ];

    match view {
        ViewMode::SubscriptionList => {
            bindings.extend(vec![
                Keybinding { key: "↑↓/jk", description: "Navigate" },
                Keybinding { key: "Enter", description: "Select" },
                Keybinding { key: "r", description: "Refresh" },
                Keybinding { key: "/", description: "Filter" },
            ]);
        }
        ViewMode::ResourceGroupList => {
            bindings.extend(vec![
                Keybinding { key: "Esc", description: "Back" },
                Keybinding { key: "↑↓/jk", description: "Navigate" },
                Keybinding { key: "Enter", description: "View Resources" },
                Keybinding { key: "r", description: "Refresh" },
            ]);
        }
        ViewMode::ResourceList => {
            bindings.extend(vec![
                Keybinding { key: "Esc", description: "Back" },
                Keybinding { key: "↑↓/jk", description: "Navigate" },
                Keybinding { key: "Enter", description: "Details" },
            ]);
            
            if has_selection {
                bindings.extend(vec![
                    Keybinding { key: "s", description: "Start" },
                    Keybinding { key: "t", description: "Stop" },
                    Keybinding { key: "d", description: "Delete" },
                ]);
            }
        }
        ViewMode::ResourceDetail => {
            bindings.extend(vec![
                Keybinding { key: "Esc", description: "Back" },
                Keybinding { key: "l", description: "Logs" },
                Keybinding { key: "m", description: "Metrics" },
            ]);
        }
        ViewMode::Logs => {
            bindings.extend(vec![
                Keybinding { key: "Esc", description: "Back" },
                Keybinding { key: "r", description: "Refresh" },
                Keybinding { key: "/", description: "Filter" },
            ]);
        }
    }

    bindings
}
```

#### 2. Update Footer Rendering

**File**: `crates/aztui-tui/src/ui/mod.rs`

```rust
fn draw_footer(f: &mut Frame, area: Rect, app: &TuiApp) {
    let has_selection = app.state().current_list_len() > 0;
    let keybindings = keybindings::get_keybindings(app.state().view, has_selection);

    let spans: Vec<Span> = keybindings
        .iter()
        .flat_map(|kb| {
            vec![
                Span::styled(kb.key, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(": "),
                Span::raw(kb.description),
                Span::raw("  "),
            ]
        })
        .collect();

    let footer = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(footer, area);
}
```

---

## Iteration 7: Multi-Column List Layouts

### Goal
Implement proper multi-column layouts with aligned data for better information density.

### Tasks

#### 1. Create Column Layout System

**File**: `crates/aztui-tui/src/ui/columns.rs` (new)

```rust
pub struct Column {
    pub name: &'static str,
    pub width: ColumnWidth,
    pub alignment: Alignment,
}

pub enum ColumnWidth {
    Fixed(u16),
    Percentage(u16),
    Flexible,
}

pub enum Alignment {
    Left,
    Right,
    Center,
}

pub struct ColumnLayout {
    columns: Vec<Column>,
}

impl ColumnLayout {
    pub fn new(columns: Vec<Column>) -> Self {
        Self { columns }
    }

    pub fn calculate_widths(&self, total_width: u16) -> Vec<u16> {
        let mut widths = vec![0; self.columns.len()];
        let mut remaining_width = total_width;
        let mut flexible_count = 0;

        // First pass: fixed and percentage widths
        for (i, col) in self.columns.iter().enumerate() {
            match col.width {
                ColumnWidth::Fixed(w) => {
                    widths[i] = w;
                    remaining_width = remaining_width.saturating_sub(w);
                }
                ColumnWidth::Percentage(p) => {
                    let w = (total_width * p) / 100;
                    widths[i] = w;
                    remaining_width = remaining_width.saturating_sub(w);
                }
                ColumnWidth::Flexible => {
                    flexible_count += 1;
                }
            }
        }

        // Second pass: distribute remaining width to flexible columns
        if flexible_count > 0 {
            let flex_width = remaining_width / flexible_count;
            for (i, col) in self.columns.iter().enumerate() {
                if matches!(col.width, ColumnWidth::Flexible) {
                    widths[i] = flex_width;
                }
            }
        }

        widths
    }

    pub fn format_row(&self, values: Vec<String>, widths: &[u16]) -> Vec<Span> {
        let mut spans = vec![];
        
        for (i, (value, width)) in values.iter().zip(widths.iter()).enumerate() {
            let col = &self.columns[i];
            let formatted = match col.alignment {
                Alignment::Left => format!("{:<width$}", truncate(value, *width as usize), width = *width as usize),
                Alignment::Right => format!("{:>width$}", truncate(value, *width as usize), width = *width as usize),
                Alignment::Center => format!("{:^width$}", truncate(value, *width as usize), width = *width as usize),
            };
            
            spans.push(Span::raw(formatted));
            if i < values.len() - 1 {
                spans.push(Span::raw(" "));
            }
        }
        
        spans
    }
}
```

#### 2. Use Column Layout in Resource List

**File**: `crates/aztui-tui/src/ui/mod.rs`

```rust
fn draw_resource_list(f: &mut Frame, area: Rect, app: &TuiApp) -> Result<()> {
    let resources = &app.state().resources;
    
    if resources.is_empty() {
        // ... empty state ...
        return Ok(());
    }

    // Define column layout
    let layout = ColumnLayout::new(vec![
        Column { name: "TYPE", width: ColumnWidth::Fixed(20), alignment: Alignment::Left },
        Column { name: "NAME", width: ColumnWidth::Flexible, alignment: Alignment::Left },
        Column { name: "LOCATION", width: ColumnWidth::Fixed(15), alignment: Alignment::Left },
        Column { name: "STATUS", width: ColumnWidth::Fixed(12), alignment: Alignment::Left },
    ]);

    let widths = layout.calculate_widths(area.width.saturating_sub(2)); // Account for borders

    // Create list items
    let items: Vec<_> = resources
        .iter()
        .map(|res| {
            let values = vec![
                res.resource_type.clone(),
                res.name.clone(),
                res.location.clone(),
                format_status(&res.provisioning_state),
            ];
            
            let spans = layout.format_row(values, &widths);
            list_item_spans(spans)
        })
        .collect();

    // ... render with SelectableList ...

    Ok(())
}

fn format_status(state: &Option<String>) -> String {
    match state.as_deref() {
        Some("Succeeded") => "● Succeeded".to_string(),
        Some("Running") => "● Running".to_string(),
        Some("Failed") => "✗ Failed".to_string(),
        Some(s) => format!("○ {}", s),
        None => "○ Unknown".to_string(),
    }
}
```

---

## Testing Strategy

### Unit Tests

**File**: `crates/aztui-tui/src/ui/columns.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_width_calculation() {
        let layout = ColumnLayout::new(vec![
            Column { name: "A", width: ColumnWidth::Fixed(10), alignment: Alignment::Left },
            Column { name: "B", width: ColumnWidth::Percentage(30), alignment: Alignment::Left },
            Column { name: "C", width: ColumnWidth::Flexible, alignment: Alignment::Left },
        ]);

        let widths = layout.calculate_widths(100);
        assert_eq!(widths[0], 10);  // Fixed
        assert_eq!(widths[1], 30);  // 30% of 100
        assert_eq!(widths[2], 60);  // Remaining
    }

    #[test]
    fn test_row_formatting() {
        let layout = ColumnLayout::new(vec![
            Column { name: "A", width: ColumnWidth::Fixed(10), alignment: Alignment::Left },
            Column { name: "B", width: ColumnWidth::Fixed(10), alignment: Alignment::Right },
        ]);

        let widths = vec![10, 10];
        let values = vec!["Left".to_string(), "Right".to_string()];
        let spans = layout.format_row(values, &widths);

        assert_eq!(spans[0].content, "Left      ");
        assert_eq!(spans[2].content, "     Right");
    }
}
```

### Integration Tests

**File**: `crates/aztui-tui/tests/navigation_tests.rs` (new)

```rust
use aztui_core::{App, state::{AppState, ViewMode}};
use aztui_tui::TuiApp;

#[tokio::test]
async fn test_subscription_to_resource_group_navigation() {
    let core_app = App::new_test(); // Test constructor
    let mut tui_app = TuiApp::new(core_app);

    // Start in subscription list
    assert_eq!(tui_app.state().view, ViewMode::SubscriptionList);

    // Simulate subscription selection
    // (requires mocking Azure API calls)
    
    // Verify view changed to resource groups
    // assert_eq!(tui_app.state().view, ViewMode::ResourceGroupList);
    
    // Verify breadcrumb updated
    // assert_eq!(tui_app.state().breadcrumb.segments.len(), 2);
}
```

---

## Performance Considerations

### Virtual Scrolling for Large Lists

**File**: `crates/aztui-tui/src/ui/widgets.rs`

Enhance `SelectableList` to only render visible items:

```rust
impl<'a> StatefulWidget for SelectableList<'a> {
    type State = SelectableListState;

    fn render(mut self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // ... existing code ...

        let list_height = list_area.height as usize;
        
        // Only iterate over visible items
        let visible_start = state.offset;
        let visible_end = (state.offset + list_height).min(self.items.len());
        
        for (i, item) in self.items[visible_start..visible_end].iter().enumerate() {
            let actual_index = visible_start + i;
            let y = list_area.y + i as u16;
            
            // ... render item ...
        }
    }
}
```

### Debounced Filtering

**File**: `crates/aztui-tui/src/app.rs`

Add debouncing for filter input:

```rust
use tokio::time::{Duration, sleep};

pub struct TuiApp {
    // ... existing fields ...
    filter_debounce: Option<tokio::task::JoinHandle<()>>,
}

async fn handle_filter_input(&mut self, input: char) {
    self.state_mut().filter.push(input);
    
    // Cancel previous debounce task
    if let Some(task) = self.filter_debounce.take() {
        task.abort();
    }
    
    // Start new debounce task
    let filter = self.state().filter.clone();
    self.filter_debounce = Some(tokio::spawn(async move {
        sleep(Duration::from_millis(300)).await;
        // Apply filter
    }));
}
```

---

## Accessibility Enhancements

### Screen Reader Announcements

**File**: `crates/aztui-tui/src/accessibility.rs` (new)

```rust
pub struct Announcer {
    last_announcement: Option<String>,
}

impl Announcer {
    pub fn announce(&mut self, message: String) {
        // In a real implementation, this would use platform-specific
        // screen reader APIs or write to a log that screen readers monitor
        self.last_announcement = Some(message);
        eprintln!("ANNOUNCE: {}", message); // Temporary implementation
    }
}

// Usage:
announcer.announce("Navigated to Resource Groups view".to_string());
announcer.announce(format!("Selected {}", resource.name));
```

---

## Next Steps

1. **Implement breadcrumb component** (Iteration 4)
2. **Add view transition logic** (Iteration 4)
3. **Enhance resource group rendering** (Iteration 4)
4. **Add status bar** (Iteration 5)
5. **Implement context-aware footer** (Iteration 6)
6. **Create column layout system** (Iteration 7)
7. **Add virtual scrolling** (Performance optimization)
8. **Implement accessibility features** (Ongoing)

---

## Success Criteria

- ✅ Breadcrumb shows current navigation path
- ✅ View transitions are smooth and preserve state
- ✅ Back navigation works correctly
- ✅ Status bar shows relevant context
- ✅ Footer hints change based on view
- ✅ Multi-column layouts are aligned and readable
- ✅ Large lists (1000+ items) scroll smoothly
- ✅ All interactions are keyboard-accessible

---

## Conclusion

This implementation guide provides concrete, actionable steps for achieving k9s-like navigation and interaction fidelity. Focus on **incremental improvements** with **continuous validation** through cargo check, clippy, and manual testing.
