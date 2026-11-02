pub mod breadcrumb;
pub mod help;
pub mod resource_type;
pub mod status_bar;
pub mod widgets;

use crate::TuiApp;
use anyhow::Result;
use az9s_core::detail;
use az9s_core::scope_tree::TreeRowKind;
use az9s_core::state::{AppState, DetailTab, ViewMode};
use breadcrumb::Breadcrumb;
use help::render_help;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, StatefulWidget, Tabs, Wrap},
    Frame,
};
use status_bar::StatusBar;
use widgets::{
    cell, list_item_spans, name_width, truncate_with_ellipsis, SelectableList, SelectableListState,
};

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
        let mut scroll = app.state().help_scroll;
        render_help(f, f.size(), &mut scroll);
        app.state_mut().help_scroll = scroll;
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
        ViewMode::ManagementGroupTree => draw_tree(f, area, app),
        ViewMode::SubscriptionList => draw_subscription_list(f, area, app),
        ViewMode::ResourceGroupList => draw_resource_group_list(f, area, app),
        ViewMode::ResourceList => draw_resource_list(f, area, app),
        ViewMode::ResourceTypeSummary => draw_type_summary(f, area, app),
        ViewMode::ResourceDetail => draw_resource_detail(f, area, app),
        ViewMode::Logs => draw_logs(f, area, app),
    }
}

/// Title suffix for resource group and resource lists: " [ALL SUBSCRIPTIONS]"
/// or " [MG: name]" for cross-subscription lists, then the active filters.
fn scope_info(state: &AppState) -> String {
    let mut info = String::new();
    if state.cross_subscription_mode {
        let group = state
            .scope_group
            .as_deref()
            .and_then(|g| state.scope_tree.as_ref()?.label_of(g));
        info = match group {
            Some(name) => format!(" [MG: {}]", truncate_with_ellipsis(name, 40)),
            None => " [ALL SUBSCRIPTIONS]".to_string(),
        };
    }
    if !state.resource_query.is_empty() {
        info.push_str(&format!(" [{}]", state.resource_query));
    }
    info
}

/// Draw the management group tree
fn draw_tree(f: &mut Frame, area: Rect, app: &mut TuiApp) -> Result<()> {
    let rows = app.state().tree_rows();

    if rows.is_empty() {
        let state = app.state();
        let message = if state.loading && state.scope_tree.is_none() {
            "Loading the management group tree...".to_string()
        } else if !state.filter.is_empty() {
            "Nothing in the tree matches the current filter.\n\nPress Esc to clear the filter."
                .to_string()
        } else {
            "No management groups loaded.\n\nPress 'r' to retry or ':sub' for subscriptions."
                .to_string()
        };
        let content = Paragraph::new(message)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Management Groups"),
            )
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true });
        f.render_widget(content, area);
        return Ok(());
    }

    let viewport = area.height.saturating_sub(2) as usize;
    let mut list_state = app.state().list_state.clone();
    let visible = list_state.visible_range(rows.len(), viewport);
    let items: Vec<_> = rows[visible.clone()]
        .iter()
        .map(|row| {
            let indent = "  ".repeat(row.depth);
            let marker = match (row.has_children, row.expanded) {
                (false, _) => "  ",
                (true, true) => "▾ ",
                (true, false) => "▸ ",
            };
            let (icon, color) = match row.kind {
                TreeRowKind::ManagementGroup => ("◆ ", Color::Cyan),
                TreeRowKind::Unplaced => ("◇ ", Color::DarkGray),
                TreeRowKind::Subscription => match row.state.as_deref() {
                    Some(s) if s.eq_ignore_ascii_case("Disabled") => ("● ", Color::Red),
                    Some(s) if s.eq_ignore_ascii_case("Warned") => ("● ", Color::Yellow),
                    _ => ("● ", Color::Green),
                },
            };
            let width = 60usize.saturating_sub(indent.chars().count() + 4).max(10);
            let label = truncate_with_ellipsis(&row.label, width);
            let detail = if row.is_group() {
                format!("{} subscriptions", row.subscription_count)
            } else {
                match row.state.as_deref() {
                    Some(s) if !s.eq_ignore_ascii_case("Enabled") => format!("{s}  {}", row.key),
                    _ => row.key.clone(),
                }
            };
            let label_style = if row.is_group() {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            list_item_spans(vec![
                Span::raw(indent),
                Span::styled(marker, Style::default().fg(Color::DarkGray)),
                Span::styled(icon, Style::default().fg(color)),
                Span::styled(format!("{label:<width$}  "), label_style),
                Span::styled(detail, Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let total = app
        .state()
        .scope_tree
        .as_ref()
        .map_or(0, |t| t.groups.len() + 1);
    let title = if app.state().filter_mode {
        format!(
            "Management Groups ({total}) - Filter: {}_",
            app.state().filter
        )
    } else if !app.state().filter.is_empty() {
        format!(
            "Management Groups ({total}) - Filtered by: {}",
            app.state().filter
        )
    } else {
        format!("Management Groups ({total}) - Enter opens resource groups, Space expands")
    };

    let list = SelectableList::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let mut widget_state = SelectableListState {
        selected: list_state.selected - visible.start,
        offset: 0,
    };
    list.render(area, f.buffer_mut(), &mut widget_state);
    app.state_mut().list_state = list_state;
    Ok(())
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
    // Fixed columns: state (12) and the subscription ID (36).
    let name_col = name_width(area.width, 12 + 36, 60);
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
                    cell(&sub.name, name_col),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(cell(&sub.state, 12), Style::default().fg(state_color)),
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
    // Fixed columns: location (20) and the tag count (~10).
    let name_col = name_width(area.width, 20 + 10, 80);
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
                    cell(&rg.name, name_col),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(cell(&rg.location, 20), Style::default().fg(Color::Yellow)),
                Span::styled(tag_info, Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    // Create title with filter indicator and scope
    let scope_info = scope_info(app.state());

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

/// Draw the resource counts per type (`:type`)
fn draw_type_summary(f: &mut Frame, area: Rect, app: &mut TuiApp) -> Result<()> {
    let rows = app.state().type_summary();
    let state = app.state();
    let scope = scope_info(state);

    if rows.is_empty() {
        let message = if state.loading {
            "Loading resources...".to_string()
        } else if !state.filter.is_empty() {
            "No resource types match the current filter.\n\nPress Esc to clear the filter."
                .to_string()
        } else {
            "No resources loaded.\n\nPress 'r' to refresh.".to_string()
        };
        let content = Paragraph::new(message)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Resource Types{scope}")),
            )
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true });
        f.render_widget(content, area);
        return Ok(());
    }

    let total: usize = rows.iter().map(|(_, n)| n).sum();
    let count_width = rows[0].1.to_string().len().max(5);
    let viewport = area.height.saturating_sub(2) as usize;
    let mut list_state = state.list_state.clone();
    let visible = list_state.visible_range(rows.len(), viewport);
    let items: Vec<_> = rows[visible.clone()]
        .iter()
        .map(|(resource_type, count)| {
            list_item_spans(vec![
                Span::styled(
                    format!("{count:>count_width$}  "),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    cell(&resource_type::short_label(resource_type), 20),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(resource_type.clone(), Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let title = if state.filter_mode {
        format!(
            "Resource Types ({}){scope} - Filter: {}_",
            rows.len(),
            state.filter
        )
    } else {
        format!(
            "Resource Types ({}, {total} resources){scope} - Enter lists them",
            rows.len()
        )
    };
    let list = SelectableList::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let mut widget_state = SelectableListState {
        selected: list_state.selected - visible.start,
        offset: 0,
    };
    list.render(area, f.buffer_mut(), &mut widget_state);
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
        } else if !app.state().resource_query.is_empty() && !app.state().resources.is_empty() {
            "No resources match the filters in the title.\n\nRun :res without filters to see all."
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
    // Fixed columns: type (20) and location (20).
    let name_col = name_width(area.width, 20 + 20, 80);
    let items: Vec<_> = filtered_resources[visible.clone()]
        .iter()
        .map(|res| {
            // Truncate long names to fit better
            let display_name = cell(&res.name, name_col);

            // Use improved resource type display
            let short_type = resource_type::short_label(&res.resource_type);
            let display_type = cell(&short_type, 20);

            // Lowercase first: Resource Graph returns types in lowercase.
            let type_color = match res.resource_type.to_ascii_lowercase().as_str() {
                t if t.contains("virtualmachine") => Color::Cyan,
                t if t.contains("storage") => Color::Yellow,
                t if t.contains("network") => Color::Blue,
                t if t.contains("database") || t.contains("sql") || t.contains("cosmos") => {
                    Color::Magenta
                }
                t if t.contains("keyvault") => Color::Red,
                t if t.contains("container") || t.contains("kubernetes") => Color::Green,
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
                Span::styled(cell(&res.location, 20), Style::default().fg(Color::Yellow)),
            ])
        })
        .collect();

    // Create title with filter indicator and scope
    let scope_info = scope_info(app.state());
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

fn draw_resource_detail(f: &mut Frame, area: Rect, app: &mut TuiApp) -> Result<()> {
    let Some(resource) = app.state().selected_resource_detail.clone() else {
        let content = Paragraph::new("No resource selected.\n\nPress Esc to go back.")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Resource Detail"),
            )
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true });
        f.render_widget(content, area);
        return Ok(());
    };
    let state = app.state();
    let doc = state.resource_document.as_ref();
    let tab = state.detail_tab;

    let lines: Vec<Line> = match tab {
        DetailTab::Overview => overview_lines(&detail::overview(doc, &resource)),
        DetailTab::Tags => {
            let tags = detail::tags(doc, &resource);
            if tags.is_empty() {
                vec![Line::from(Span::styled(
                    "No tags.",
                    Style::default().fg(Color::DarkGray),
                ))]
            } else {
                let width = tags
                    .iter()
                    .map(|(k, _)| k.chars().count())
                    .max()
                    .unwrap_or(0);
                tags.into_iter()
                    .map(|(k, v)| {
                        Line::from(vec![
                            Span::styled(
                                format!("{k:<width$}  "),
                                Style::default().fg(Color::Yellow),
                            ),
                            Span::styled(v, Style::default().fg(Color::White)),
                        ])
                    })
                    .collect()
            }
        }
        DetailTab::Json => detail::json_text(doc).lines().map(json_line).collect(),
    };
    let loading_hint = if doc.is_none() && state.loading {
        " · loading details…"
    } else {
        ""
    };

    let block = Block::default().borders(Borders::ALL).title(format!(
        "{} · {}{}",
        truncate_with_ellipsis(&resource.name, 60),
        resource_type::short_label(&resource.resource_type),
        loading_hint
    ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(inner);

    let titles: Vec<Line> = DetailTab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| Line::from(format!("{} {}", i + 1, t.title())))
        .collect();
    let tabs = Tabs::new(titles)
        .select(tab.index())
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider("│");
    f.render_widget(tabs, parts[0]);

    // Clamp the scroll so the last line can reach the bottom but not beyond.
    let viewport = parts[1].height as usize;
    let max_scroll = lines.len().saturating_sub(viewport);
    let scroll = state.detail_scroll.min(max_scroll);
    let content = Paragraph::new(lines).scroll((scroll.min(u16::MAX as usize) as u16, 0));
    f.render_widget(content, parts[1]);
    app.state_mut().detail_scroll = scroll;
    Ok(())
}

/// The Overview tab: a heading per section, then aligned label/value rows.
fn overview_lines(sections: &[detail::Section]) -> Vec<Line<'static>> {
    let width = sections
        .iter()
        .flat_map(|s| s.rows.iter().map(|(l, _)| l.chars().count()))
        .max()
        .unwrap_or(0)
        .min(40);
    let mut lines = Vec::new();
    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            section.title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        for (label, value) in &section.rows {
            let label = truncate_with_ellipsis(label, width);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {label:<width$}  "),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(value.clone(), Style::default().fg(Color::White)),
            ]));
        }
    }
    lines
}

/// One line of pretty-printed JSON with keys and values coloured.
fn json_line(line: &str) -> Line<'static> {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    let mut spans = vec![Span::raw(indent.to_string())];
    // `"key": value` — split after the key's closing quote and colon.
    let value = match rest.find("\": ") {
        Some(i) if rest.starts_with('"') => {
            spans.push(Span::styled(
                rest[..i + 1].to_string(),
                Style::default().fg(Color::Cyan),
            ));
            spans.push(Span::raw(": "));
            &rest[i + 3..]
        }
        _ => rest,
    };
    let bare = value.trim_end_matches(',');
    let color = if bare.starts_with('"') {
        Color::Green
    } else if bare == "true" || bare == "false" || bare == "null" {
        Color::Magenta
    } else if bare.starts_with(|c: char| c == '-' || c.is_ascii_digit()) {
        Color::Yellow
    } else {
        Color::Gray
    };
    spans.push(Span::styled(value.to_string(), Style::default().fg(color)));
    Line::from(spans)
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
            ViewMode::ManagementGroupTree => vec![
                ("↑↓/jk", "Navigate"),
                ("Space", "Expand"),
                ("←→/hl", "Close/Open"),
                ("Enter", "Resource Groups"),
                ("/", "Filter"),
                (":", "Command"),
                ("?", "Help"),
                ("r", "Refresh"),
                ("q", "Quit"),
            ],
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
            ViewMode::ResourceTypeSummary => vec![
                ("↑↓/jk", "Navigate"),
                ("Enter", "List Resources"),
                ("/", "Filter"),
                (":", "Command"),
                ("?", "Help"),
                ("r", "Refresh"),
                ("Esc", "Back"),
                ("q", "Quit"),
            ],
            ViewMode::ResourceDetail => vec![
                ("Tab/1-3", "Tabs"),
                ("↑↓/jk", "Scroll"),
                ("y", "Copy ID"),
                ("o", "Portal"),
                ("r", "Refresh"),
                ("Esc", "Back"),
                ("q", "Quit"),
            ],
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
