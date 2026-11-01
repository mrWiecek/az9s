pub mod breadcrumb;
pub mod help;
pub mod resource_type;
pub mod status_bar;
pub mod widgets;

use crate::TuiApp;
use anyhow::Result;
use az9s_core::state::ViewMode;
use breadcrumb::Breadcrumb;
use help::render_help;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, StatefulWidget, Wrap},
    Frame,
};
use status_bar::StatusBar;
use widgets::{list_item_spans, truncate_with_ellipsis, SelectableList, SelectableListState};

/// Draw the main UI
///
/// This is the main entry point for rendering the TUI. It divides the screen
/// into header, content, and footer sections, and delegates to view-specific
/// rendering functions.
pub fn draw(f: &mut Frame, app: &mut TuiApp) -> Result<()> {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header (breadcrumb)
            Constraint::Min(0),    // Main content
            Constraint::Length(3), // Status bar
            Constraint::Length(3), // Footer (keybindings/command)
        ])
        .split(f.size());

    draw_header(f, chunks[0], app);
    draw_content(f, chunks[1], app)?;
    draw_status_bar(f, chunks[2], app);
    draw_footer(f, chunks[3], app);

    // Draw error overlay if present
    if app.state().error.is_some() {
        draw_error_overlay(f, app);
    }

    // Draw help screen if open
    if app.state().help_open {
        render_help(f, f.size());
    }

    Ok(())
}

/// Draw the header with breadcrumb navigation
fn draw_header(f: &mut Frame, area: Rect, app: &TuiApp) {
    let breadcrumb_line = Breadcrumb::render(app.state());

    let header = Paragraph::new(breadcrumb_line).block(Block::default().borders(Borders::ALL));

    f.render_widget(header, area);
}

/// Draw the status bar with connection and context information
fn draw_status_bar(f: &mut Frame, area: Rect, app: &TuiApp) {
    let status_line = StatusBar::render(app.state());

    let status = Paragraph::new(status_line).block(Block::default().borders(Borders::ALL));

    f.render_widget(status, area);
}

/// Draw the main content area based on current view
fn draw_content(f: &mut Frame, area: Rect, app: &mut TuiApp) -> Result<()> {
    match app.state().view {
        ViewMode::SubscriptionList => draw_subscription_list(f, area, app),
        ViewMode::ResourceGroupList => draw_resource_group_list(f, area, app),
        ViewMode::ResourceList => draw_resource_list(f, area, app),
        ViewMode::ResourceDetail => draw_resource_detail(f, area, app),
        ViewMode::Logs => draw_logs(f, area, app),
    }
}

/// Draw subscription list view
fn draw_subscription_list(f: &mut Frame, area: Rect, app: &mut TuiApp) -> Result<()> {
    let filtered_subscriptions = app.state().filtered_subscriptions();

    if filtered_subscriptions.is_empty() {
        let message = if app.state().loading {
            "Loading subscriptions...\n\nPlease wait while we fetch your Azure subscriptions."
        } else if !app.state().filter.is_empty() {
            "No subscriptions match the current filter.\n\nPress Esc to clear the filter."
        } else {
            "No subscriptions loaded.\n\nPress 'r' to refresh or check your Azure authentication."
        };

        let content = Paragraph::new(message)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Subscriptions"),
            )
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true });
        f.render_widget(content, area);
        return Ok(());
    }

    // Create list items with color-coded state
    // Only build widgets for the rows that fit on screen (minus the borders).
    let viewport = area.height.saturating_sub(2) as usize;
    let mut list_state = app.state().list_state.clone();
    let visible = list_state.visible_range(filtered_subscriptions.len(), viewport);
    let items: Vec<_> = filtered_subscriptions[visible.clone()]
        .iter()
        .map(|sub| {
            let state_color = match sub.state.as_str() {
                "Enabled" => Color::Green,
                "Disabled" => Color::Red,
                "Warned" => Color::Yellow,
                _ => Color::Gray,
            };

            list_item_spans(vec![
                Span::styled(
                    format!("{:<40}", sub.name),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<12}", sub.state),
                    Style::default().fg(state_color),
                ),
                Span::styled(sub.id.clone(), Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    // Create title with filter indicator
    let title = if app.state().filter_mode {
        format!(
            "Subscriptions ({}) - Filter: {}_",
            filtered_subscriptions.len(),
            app.state().filter
        )
    } else if !app.state().filter.is_empty() {
        format!(
            "Subscriptions ({}) - Filtered by: {}",
            filtered_subscriptions.len(),
            app.state().filter
        )
    } else {
        format!(
            "Subscriptions ({}) - Press Enter to select",
            filtered_subscriptions.len()
        )
    };

    // Create the list widget
    let list = SelectableList::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    // Items are already windowed: render from the top with a relative selection.
    let mut widget_state = SelectableListState {
        selected: list_state.selected - visible.start,
        offset: 0,
    };
    list.render(area, f.buffer_mut(), &mut widget_state);

    // Persist the scroll offset so scrolling up doesn't snap the selection.
    app.state_mut().list_state = list_state;

    Ok(())
}

/// Draw resource group list view
fn draw_resource_group_list(f: &mut Frame, area: Rect, app: &mut TuiApp) -> Result<()> {
    let filtered_resource_groups = app.state().filtered_resource_groups();

    if filtered_resource_groups.is_empty() {
        let message = if app.state().loading {
            "Loading resource groups...\n\nPlease wait while we fetch resource groups from Azure."
        } else if !app.state().filter.is_empty() {
            "No resource groups match the current filter.\n\nPress Esc to clear the filter."
        } else {
            "No resource groups loaded.\n\nSelect a subscription to view resource groups.\nPress 'r' to refresh."
        };

        let content = Paragraph::new(message)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Resource Groups"),
            )
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true });
        f.render_widget(content, area);
        return Ok(());
    }

    // Create list items for resource groups
    // Only build widgets for the rows that fit on screen (minus the borders).
    let viewport = area.height.saturating_sub(2) as usize;
    let mut list_state = app.state().list_state.clone();
    let visible = list_state.visible_range(filtered_resource_groups.len(), viewport);
    let items: Vec<_> = filtered_resource_groups[visible.clone()]
        .iter()
        .map(|rg| {
            // Display name, location, and tag count
            let tag_count = rg.tags.len();
            let tag_info = if tag_count > 0 {
                format!("{} tags", tag_count)
            } else {
                "no tags".to_string()
            };

            list_item_spans(vec![
                Span::styled(
                    format!("{:<40}", rg.name),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<20}", rg.location),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(tag_info, Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    // Create title with filter indicator and scope
    let scope_info = if app.state().cross_subscription_mode {
        " [ALL SUBSCRIPTIONS]"
    } else {
        ""
    };

    let title = if app.state().filter_mode {
        format!(
            "Resource Groups ({}){}  - Filter: {}_",
            filtered_resource_groups.len(),
            scope_info,
            app.state().filter
        )
    } else if !app.state().filter.is_empty() {
        format!(
            "Resource Groups ({}){}  - Filtered by: {}",
            filtered_resource_groups.len(),
            scope_info,
            app.state().filter
        )
    } else {
        format!(
            "Resource Groups ({}){}  - Press Enter to view resources",
            filtered_resource_groups.len(),
            scope_info
        )
    };

    // Create the list widget
    let list = SelectableList::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    // Items are already windowed: render from the top with a relative selection.
    let mut widget_state = SelectableListState {
        selected: list_state.selected - visible.start,
        offset: 0,
    };
    list.render(area, f.buffer_mut(), &mut widget_state);

    // Persist the scroll offset so scrolling up doesn't snap the selection.
    app.state_mut().list_state = list_state;

    Ok(())
}

/// Draw resource list view
fn draw_resource_list(f: &mut Frame, area: Rect, app: &mut TuiApp) -> Result<()> {
    let filtered_resources = app.state().filtered_resources();

    if filtered_resources.is_empty() {
        let message = if app.state().loading {
            "Loading resources...\n\nPlease wait while we fetch resources from Azure."
        } else if !app.state().filter.is_empty() {
            "No resources match the current filter.\n\nPress Esc to clear the filter."
        } else {
            "No resources loaded.\n\nSelect a subscription to view resources.\nPress 'r' to refresh."
        };

        let content = Paragraph::new(message)
            .block(Block::default().borders(Borders::ALL).title("Resources"))
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true });
        f.render_widget(content, area);
        return Ok(());
    }

    // Create list items with improved resource type display
    // Only build widgets for the rows that fit on screen (minus the borders).
    let viewport = area.height.saturating_sub(2) as usize;
    let mut list_state = app.state().list_state.clone();
    let visible = list_state.visible_range(filtered_resources.len(), viewport);
    let items: Vec<_> = filtered_resources[visible.clone()]
        .iter()
        .map(|res| {
            // Truncate long names to fit better
            let display_name = format!("{:<35}", truncate_with_ellipsis(&res.name, 35));

            // Use improved resource type display
            let short_type = resource_type::short_label(&res.resource_type);
            let display_type = format!("{:<20}", short_type);

            let type_color = match res.resource_type.as_str() {
                t if t.contains("VirtualMachine") => Color::Cyan,
                t if t.contains("Storage") => Color::Yellow,
                t if t.contains("Network") => Color::Blue,
                t if t.contains("Database") || t.contains("Sql") || t.contains("Cosmos") => {
                    Color::Magenta
                }
                t if t.contains("KeyVault") => Color::Red,
                t if t.contains("Container") || t.contains("Kubernetes") => Color::Green,
                _ => Color::White,
            };

            list_item_spans(vec![
                Span::styled(
                    display_name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(display_type, Style::default().fg(type_color)),
                Span::styled(
                    format!("{:<12}", res.location),
                    Style::default().fg(Color::Yellow),
                ),
            ])
        })
        .collect();

    // Create title with filter indicator and scope
    let scope_info = if app.state().cross_subscription_mode {
        " [ALL SUBSCRIPTIONS]"
    } else {
        ""
    };

    let title = if app.state().filter_mode {
        format!(
            "Resources ({}){}  - Filter: {}_",
            filtered_resources.len(),
            scope_info,
            app.state().filter
        )
    } else if !app.state().filter.is_empty() {
        format!(
            "Resources ({}){}  - Filtered by: {}",
            filtered_resources.len(),
            scope_info,
            app.state().filter
        )
    } else {
        format!(
            "Resources ({}){}  - Press Enter for details",
            filtered_resources.len(),
            scope_info
        )
    };

    // Create the list widget
    let list = SelectableList::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    // Items are already windowed: render from the top with a relative selection.
    let mut widget_state = SelectableListState {
        selected: list_state.selected - visible.start,
        offset: 0,
    };
    list.render(area, f.buffer_mut(), &mut widget_state);

    // Persist the scroll offset so scrolling up doesn't snap the selection.
    app.state_mut().list_state = list_state;

    Ok(())
}

/// Draw resource detail view
fn draw_resource_detail(f: &mut Frame, area: Rect, app: &TuiApp) -> Result<()> {
    if let Some(resource) = &app.state().selected_resource_detail {
        // Create detailed view with resource properties
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    "Name: ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(&resource.name, Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "Type: ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(&resource.resource_type, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  Short: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    resource_type::short_label(&resource.resource_type),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "Location: ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(&resource.location, Style::default().fg(Color::Yellow)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "Resource Group: ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(&resource.resource_group, Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "ID: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(Span::styled(
                &resource.id,
                Style::default().fg(Color::DarkGray),
            )),
        ];

        // Add tags section if present
        if !resource.tags.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                "Tags:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]));
            for (key, value) in &resource.tags {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(format!("{}: ", key), Style::default().fg(Color::Yellow)),
                    Span::styled(value, Style::default().fg(Color::White)),
                ]));
            }
        } else {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "Tags: ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("none", Style::default().fg(Color::DarkGray)),
            ]));
        }

        let content = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Resource Detail"),
            )
            .wrap(Wrap { trim: true });
        f.render_widget(content, area);
    } else {
        let content = Paragraph::new("No resource selected.\n\nPress Esc to go back.")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Resource Detail"),
            )
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true });
        f.render_widget(content, area);
    }
    Ok(())
}

/// Draw logs view
fn draw_logs(f: &mut Frame, area: Rect, _app: &TuiApp) -> Result<()> {
    let content =
        Paragraph::new("Logs view\n\nSelect a resource to view its logs.\n\nPress Esc to go back.")
            .block(Block::default().borders(Borders::ALL).title("Logs"))
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true });
    f.render_widget(content, area);
    Ok(())
}

/// Draw the footer with keybindings or command input
fn draw_footer(f: &mut Frame, area: Rect, app: &TuiApp) {
    // If in command mode, show command input
    if app.state().command_mode {
        let command_text = format!(":{}_", app.state().command_input);
        let footer = Paragraph::new(command_text)
            .block(Block::default().borders(Borders::ALL).title("Command"))
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(footer, area);
        return;
    }

    let keybindings = if app.state().filter_mode {
        vec![
            ("Type", "Filter"),
            ("Enter", "Apply"),
            ("Esc", "Cancel"),
            ("Backspace", "Delete"),
        ]
    } else if app.state().help_open {
        vec![("Esc/?/q", "Close Help")]
    } else {
        match app.state().view {
            ViewMode::SubscriptionList => vec![
                ("↑↓/jk", "Navigate"),
                ("g/G", "First/Last"),
                ("PgUp/PgDn", "Page"),
                ("Enter", "Select"),
                ("/", "Filter"),
                (":", "Command"),
                ("?", "Help"),
                ("r", "Refresh"),
                ("q", "Quit"),
            ],
            ViewMode::ResourceGroupList => vec![
                ("↑↓/jk", "Navigate"),
                ("g/G", "First/Last"),
                ("PgUp/PgDn", "Page"),
                ("Enter", "View Resources"),
                ("/", "Filter"),
                (":", "Command"),
                ("?", "Help"),
                ("r", "Refresh"),
                ("Esc", "Back"),
                ("q", "Quit"),
            ],
            ViewMode::ResourceList => vec![
                ("↑↓/jk", "Navigate"),
                ("g/G", "First/Last"),
                ("PgUp/PgDn", "Page"),
                ("Enter", "Details"),
                ("/", "Filter"),
                (":", "Command"),
                ("?", "Help"),
                ("r", "Refresh"),
                ("Esc", "Back"),
                ("q", "Quit"),
            ],
            ViewMode::ResourceDetail => vec![("Esc", "Back"), ("?", "Help"), ("q", "Quit")],
            _ => vec![
                ("↑↓/jk", "Navigate"),
                ("Enter", "Select"),
                ("/", "Filter"),
                (":", "Command"),
                ("?", "Help"),
                ("Esc", "Back"),
                ("r", "Refresh"),
                ("q", "Quit"),
            ],
        }
    };

    let spans: Vec<Span> = keybindings
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(
                    format!(" {} ", key),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {} ", desc), Style::default().fg(Color::White)),
            ]
        })
        .collect();

    let footer = Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::ALL));

    f.render_widget(footer, area);
}

/// Draw error overlay
fn draw_error_overlay(f: &mut Frame, app: &TuiApp) {
    if let Some(error) = &app.state().error {
        let area = centered_rect(60, 20, f.size());

        let error_text = format!("Error\n\n{}\n\nPress Esc to dismiss", error);
        let error_widget = Paragraph::new(error_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Error")
                    .style(Style::default().fg(Color::Red)),
            )
            .style(Style::default().fg(Color::White).bg(Color::Black))
            .wrap(Wrap { trim: true });

        f.render_widget(error_widget, area);
    }
}

/// Helper function to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ui_module_exists() {
        // Basic test to ensure module compiles
    }
}
