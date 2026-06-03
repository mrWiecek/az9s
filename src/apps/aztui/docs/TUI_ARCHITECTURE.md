# aztui TUI Architecture

## Overview

The aztui TUI (Terminal User Interface) is built using **Ratatui** and **Crossterm**, providing a responsive, keyboard-driven interface for browsing Azure resources. This document describes the architecture, event handling, rendering pipeline, and navigation system.

## Architecture Components

### 1. Terminal Lifecycle Management

**Location**: `crates/aztui-tui/src/lib.rs`

The TUI manages the terminal lifecycle through these phases:

```rust
pub async fn run(core_app: App) -> Result<()> {
    // 1. Setup: Enable raw mode and alternate screen
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    
    // 2. Create terminal backend
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // 3. Run event loop
    run_loop(&mut terminal, &mut tui_app, &mut event_handler).await?;
    
    // 4. Cleanup: Restore terminal state
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
}
```

**Key Features**:
- **Raw mode**: Disables line buffering for immediate key response
- **Alternate screen**: Preserves terminal history
- **Mouse capture**: Enables mouse events (future enhancement)
- **Graceful cleanup**: Always restores terminal state, even on error

### 2. Event Loop

**Location**: `crates/aztui-tui/src/lib.rs::run_loop()`

The event loop follows a simple pattern:

```
┌─────────────────────────────────────┐
│  1. Draw UI (terminal.draw())       │
│     - Render all widgets            │
│     - Update screen buffer          │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│  2. Wait for Event                  │
│     - Keyboard input                │
│     - Tick (periodic update)        │
│     - Resize                        │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│  3. Handle Event                    │
│     - Update application state      │
│     - Return continue/quit signal   │
└──────────────┬──────────────────────┘
               │
               ▼
          Loop or Exit
```

**Performance Characteristics**:
- **Non-blocking**: Uses tokio::select! for async event handling
- **Tick rate**: 250ms default (configurable)
- **Responsive**: Keyboard events processed immediately

### 3. Event Handler

**Location**: `crates/aztui-tui/src/event.rs`

The event handler bridges Crossterm events to application events:

```rust
pub enum Event {
    Key(KeyEvent),      // Keyboard input
    Tick,               // Periodic update
    Resize(u16, u16),   // Terminal resize
}
```

**Implementation**:
- Uses `tokio::select!` to multiplex between tick timer and terminal events
- Polls terminal with zero timeout to avoid blocking
- Converts Crossterm events to application events

### 4. Application State

**Location**: `crates/aztui-tui/src/app.rs`

The `TuiApp` wraps the core application and manages UI-specific state:

```rust
pub struct TuiApp {
    core: App,           // Core business logic
    should_quit: bool,   // Quit flag
}
```

**Responsibilities**:
- Route keyboard events to appropriate handlers
- Manage navigation state
- Coordinate between UI and core logic
- Handle global keybindings

### 5. Keyboard Handling

**Location**: `crates/aztui-tui/src/app.rs::handle_key()`

Keyboard handling is hierarchical:

```
Global Keybindings (always active)
├── Ctrl+C / Ctrl+Q → Quit
├── Ctrl+P → Command palette
├── ? → Help
├── Esc → Back/Close
└── / → Filter

Navigation Keybindings (list views)
├── ↑/k → Previous item
├── ↓/j → Next item
├── g → First item
├── G → Last item
├── PgUp/PgDn → Page navigation
└── Enter → Select item

View-Specific Keybindings
└── Handled by view-specific handlers
```

**Key Features**:
- **Vim-style navigation**: j/k for up/down, g/G for first/last
- **Arrow keys**: Standard navigation
- **Modifier support**: Ctrl, Alt, Shift
- **Context-aware**: Different keys in different views

### 6. List Navigation System

**Location**: `crates/aztui-core/src/state.rs::ListState`

The list navigation system provides:

```rust
pub struct ListState {
    selected: usize,  // Currently selected index
    offset: usize,    // Scroll offset for viewport
}
```

**Features**:
- **Bounds checking**: Prevents out-of-range selection
- **Automatic scrolling**: Keeps selected item visible
- **Viewport management**: Efficient rendering of large lists
- **Empty list handling**: Safe operations on empty lists

**Navigation Methods**:
- `select_next(list_len)` - Move down
- `select_previous()` - Move up
- `select_first()` - Jump to top
- `select_last(list_len)` - Jump to bottom
- `update_offset(viewport_height)` - Adjust scroll position

### 7. Rendering Pipeline

**Location**: `crates/aztui-tui/src/ui/mod.rs`

The rendering pipeline is organized hierarchically:

```
draw()
├── draw_header()      - Title, status, loading indicator
├── draw_content()     - Main view area
│   ├── draw_subscription_list()
│   ├── draw_resource_group_list()
│   ├── draw_resource_list()
│   ├── draw_resource_detail()
│   └── draw_logs()
├── draw_footer()      - Keybinding hints
└── draw_error_overlay() - Error dialog (if present)
```

**Layout Structure**:
```
┌─────────────────────────────────────┐
│ Header (3 lines)                    │
│ aztui | View Name [⟳]               │
├─────────────────────────────────────┤
│                                     │
│                                     │
│ Content Area (flexible)             │
│                                     │
│                                     │
├─────────────────────────────────────┤
│ Footer (3 lines)                    │
│ q: Quit  ↑↓/jk: Navigate  r: Refresh│
└─────────────────────────────────────┘
```

### 8. Custom Widgets

**Location**: `crates/aztui-tui/src/ui/widgets.rs`

#### SelectableList Widget

A custom widget for rendering lists with selection highlighting:

```rust
pub struct SelectableList<'a> {
    items: Vec<ListItem<'a>>,
    block: Option<Block<'a>>,
    highlight_style: Style,
    normal_style: Style,
}
```

**Features**:
- **Stateful rendering**: Maintains selection and scroll state
- **Automatic scrolling**: Adjusts viewport to show selected item
- **Style customization**: Configurable highlight and normal styles
- **Efficient rendering**: Only renders visible items

**Usage**:
```rust
let items = vec![
    list_item_spans(vec![
        Span::styled("Item 1", Style::default().fg(Color::Green)),
        Span::raw(" - Details"),
    ]),
];

let list = SelectableList::new(items)
    .block(Block::default().borders(Borders::ALL).title("Items"))
    .highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black));

let mut state = SelectableListState { selected: 0, offset: 0 };
list.render(area, buf, &mut state);
```

## View System

### View Modes

**Location**: `crates/aztui-core/src/state.rs::ViewMode`

```rust
pub enum ViewMode {
    SubscriptionList,    // Browse subscriptions
    ResourceGroupList,   // Browse resource groups
    ResourceList,        // Browse resources
    ResourceDetail,      // View resource details
    Logs,               // View resource logs
}
```

### View-Specific Rendering

Each view has its own rendering function:

#### 1. Subscription List
- Displays all Azure subscriptions
- Shows subscription name and state
- Supports selection and navigation
- Empty state: "No subscriptions loaded"

#### 2. Resource Group List
- Shows resource groups in selected subscription
- Displays location and tags
- Placeholder: "Select a subscription"

#### 3. Resource List
- Lists resources with type and location
- Color-coded by resource type
- Supports filtering (future)
- Empty state: "No resources loaded"

#### 4. Resource Detail
- Shows detailed resource properties
- JSON view toggle (future)
- Tags and metadata display
- Placeholder: "Select a resource"

#### 5. Logs View
- Displays resource logs
- Auto-refresh support (future)
- Log filtering (future)
- Placeholder: "Select a resource"

## State Management

### Application State

**Location**: `crates/aztui-core/src/state.rs::AppState`

```rust
pub struct AppState {
    view: ViewMode,                          // Current view
    selected_subscription: Option<Subscription>,
    subscriptions: Vec<Subscription>,
    resources: Vec<Resource>,
    navigation_stack: Vec<NavigationContext>,
    command_palette_open: bool,
    filter: String,
    list_state: ListState,                   // Selection state
    loading: bool,                           // Loading indicator
    error: Option<String>,                   // Error message
}
```

### State Updates

State updates follow a unidirectional flow:

```
Event → Handler → State Update → Render
```

**Example**: Navigating down in a list
```rust
// 1. Event: User presses 'j'
Event::Key(KeyEvent { code: KeyCode::Char('j'), .. })

// 2. Handler: Update list state
fn navigate_down(&mut self) {
    let state = self.state_mut();
    let list_len = state.current_list_len();
    state.list_state.select_next(list_len);
}

// 3. Render: UI reflects new selection
draw_subscription_list(f, area, app)
```

## Error Handling

### Error Display

Errors are displayed as modal overlays:

```
┌──────────────────────────────────┐
│ Error                            │
│                                  │
│ Failed to fetch subscriptions    │
│ Authentication failed            │
│                                  │
│ Press Esc to dismiss             │
└──────────────────────────────────┘
```

**Features**:
- **Non-blocking**: User can dismiss and continue
- **Centered**: Overlay centered on screen
- **Styled**: Red border and text
- **Dismissible**: Esc key to close

### Error Flow

```rust
// 1. Set error in state
state.set_error(Some("Authentication failed".to_string()));

// 2. Render error overlay
if app.state().error.is_some() {
    draw_error_overlay(f, app);
}

// 3. User dismisses with Esc
KeyCode::Esc => {
    state.clear_error();
}
```

## Loading States

Loading indicators are shown in the header:

```
aztui | Subscriptions [⟳]
```

**Implementation**:
```rust
if app.state().loading {
    spans.push(Span::styled("⟳", Style::default().fg(Color::Green)));
}
```

## Performance Considerations

### Rendering Optimization

1. **Viewport Culling**: Only render visible list items
2. **Incremental Updates**: Ratatui's diffing algorithm
3. **Efficient Layouts**: Constraint-based layout system
4. **Minimal Redraws**: Only redraw on state change

### Memory Management

1. **Bounded Lists**: Pagination for large datasets (future)
2. **String Interning**: Reuse common strings
3. **Lazy Loading**: Load data on demand
4. **Cache Management**: SQLite cache for offline access

### Responsiveness

1. **Non-blocking I/O**: All Azure API calls are async
2. **Immediate Feedback**: UI updates before API calls complete
3. **Background Refresh**: Periodic updates without blocking UI
4. **Tick Rate**: 250ms balance between responsiveness and CPU usage

## Testing Strategy

### Unit Tests

**Location**: `crates/aztui-core/src/state.rs`

Tests for list navigation logic:
- `test_list_state_navigation` - Basic navigation
- `test_list_state_bounds` - Boundary conditions
- `test_list_state_empty_list` - Empty list handling
- `test_list_state_reset` - State reset
- `test_list_state_offset_update` - Scroll offset calculation

### Integration Tests

Future tests will cover:
- End-to-end event handling
- View transitions
- Error handling flows
- Keyboard navigation scenarios

### Manual Testing

Test scenarios:
1. Navigate through long lists (100+ items)
2. Resize terminal during operation
3. Rapid key presses
4. Error conditions (network failure, auth failure)
5. Empty states (no subscriptions, no resources)

## Future Enhancements

### Planned Features

1. **Command Palette** (Iteration 8)
   - Fuzzy search for commands
   - Keyboard shortcuts
   - Command history

2. **Multi-Pane Layout** (Iteration 11)
   - Split views (horizontal/vertical)
   - Focus management
   - Pane resize

3. **Search and Filtering** (Iteration 7)
   - Fuzzy search
   - Filter by type, location, tags
   - Search history

4. **Help System** (Iteration 9)
   - Context-aware help
   - Keybinding reference
   - Getting started guide

5. **Themes** (Iteration 13)
   - Color scheme customization
   - Dark/light mode
   - User-defined themes

### Performance Improvements

1. **Virtual Scrolling**: Handle 10,000+ items efficiently
2. **Incremental Loading**: Load data in chunks
3. **Background Refresh**: Auto-update without blocking
4. **Caching Strategy**: Intelligent cache invalidation

## Troubleshooting

### Common Issues

#### Terminal Not Restored
**Symptom**: Terminal in raw mode after crash
**Solution**: Run `reset` command or restart terminal

#### Flickering
**Symptom**: Screen flickers during updates
**Solution**: Reduce tick rate or optimize rendering

#### Slow Navigation
**Symptom**: Lag when navigating large lists
**Solution**: Implement virtual scrolling (Iteration 10)

#### Keys Not Working
**Symptom**: Some keys don't respond
**Solution**: Check terminal emulator compatibility

## References

- [Ratatui Documentation](https://ratatui.rs/)
- [Crossterm Documentation](https://docs.rs/crossterm/)
- [k9s](https://k9scli.io/) - Inspiration for keyboard navigation
- [Tokio Async Runtime](https://tokio.rs/)

## Conclusion

The aztui TUI provides a responsive, keyboard-driven interface for Azure resource management. The architecture emphasizes:

- **Separation of concerns**: UI, state, and business logic are decoupled
- **Async-first**: Non-blocking I/O for responsiveness
- **Testability**: Unit tests for core logic
- **Extensibility**: Easy to add new views and widgets
- **Performance**: Efficient rendering and memory usage

The foundation is solid for building advanced features in future iterations.
