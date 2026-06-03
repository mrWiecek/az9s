pub mod widgets;

use crate::TuiApp;
use anyhow::Result;
use aztui_core::state::ViewMode;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, StatefulWidget},
    Frame,
};
use widgets::{list_item_spans, SelectableList, SelectableListState};

/// Draw the main UI
pub fn draw(f: &mut Frame, app: &TuiApp) -> Result<()> {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Main content
            Constraint::Length(3),  // Footer
        ])
        .split(f.size());

    draw_header(f, chunks[0], app);
    draw_content(f, chunks[1], app)?;
    draw_footer(f, chunks[2], app);

    // Draw error overlay if present
    if app.state().error.is_some() {
        draw_error_overlay(f, app);
    }

    Ok(())
}

/// Draw the header
fn draw_header(f: &mut Frame, area: Rect, app: &TuiApp) {
    let title = match app.state().view {
        ViewMode::SubscriptionList => "Subscriptions",
        ViewMode::ResourceGroupList => "Resource Groups",
        ViewMode::ResourceList => "Resources",
        ViewMode::ResourceDetail => "Resource Detail",
        ViewMode::Logs => "Logs",
    };

    let mut spans = vec![
        Span::styled("aztui", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        Span::styled(title, Style::default().fg(Color::Yellow)),
    ];

    // Add loading indicator
    if app.state().loading {
        spans.push(Span::raw(" "));
        spans.push(Span::styled("⟳", Style::default().fg(Color::Green)));
    }

    let header = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(header, area);
}

/// Draw the main content area
fn draw_content(f: &mut Frame, area: Rect, app: &TuiApp) -> Result<()> {
    match app.state().view {
        ViewMode::SubscriptionList => draw_subscription_list(f, area, app),
        ViewMode::ResourceGroupList => draw_resource_group_list(f, area, app),
        ViewMode::ResourceList => draw_resource_list(f, area, app),
        ViewMode::ResourceDetail => draw_resource_detail(f, area, app),
        ViewMode::Logs => draw_logs(f, area, app),
    }
}

/// Draw subscription list view
fn draw_subscription_list(f: &mut Frame, area: Rect, app: &TuiApp) -> Result<()> {
    let subscriptions = &app.state().subscriptions;
    
    if subscriptions.is_empty() {
        let content = Paragraph::new("No subscriptions loaded.\n\nPress 'r' to refresh.")
            .block(Block::default().borders(Borders::ALL).title("Subscriptions"))
            .style(Style::default().fg(Color::Gray));
        f.render_widget(content, area);
        return Ok(());
    }

    // Create list items
    let items: Vec<_> = subscriptions
        .iter()
        .map(|sub| {
            list_item_spans(vec![
                Span::styled(&sub.name, Style::default().fg(Color::Green)),
                Span::raw(" "),
                Span::styled(&sub.state, Style::default().fg(Color::Gray)),
            ])
        })
        .collect();

    // Create the list widget
    let list = SelectableList::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Subscriptions ({})", subscriptions.len()))
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        );

    // Create state from app state
    let mut list_state = SelectableListState {
        selected: app.state().list_state.selected,
        offset: app.state().list_state.offset,
    };

    // Render the list
    list.render(area, f.buffer_mut(), &mut list_state);

    Ok(())
}

/// Draw resource group list view
fn draw_resource_group_list(f: &mut Frame, area: Rect, _app: &TuiApp) -> Result<()> {
    let content = Paragraph::new("Resource Groups view\n\nSelect a subscription to view its resource groups.")
        .block(Block::default().borders(Borders::ALL).title("Resource Groups"))
        .style(Style::default().fg(Color::Gray));
    f.render_widget(content, area);
    Ok(())
}

/// Draw resource list view
fn draw_resource_list(f: &mut Frame, area: Rect, app: &TuiApp) -> Result<()> {
    let resources = &app.state().resources;
    
    if resources.is_empty() {
        let content = Paragraph::new("No resources loaded.\n\nSelect a subscription or resource group to view resources.")
            .block(Block::default().borders(Borders::ALL).title("Resources"))
            .style(Style::default().fg(Color::Gray));
        f.render_widget(content, area);
        return Ok(());
    }

    // Create list items
    let items: Vec<_> = resources
        .iter()
        .map(|res| {
            list_item_spans(vec![
                Span::styled(&res.name, Style::default().fg(Color::Green)),
                Span::raw(" "),
                Span::styled(&res.resource_type, Style::default().fg(Color::Cyan)),
                Span::raw(" "),
                Span::styled(&res.location, Style::default().fg(Color::Yellow)),
            ])
        })
        .collect();

    // Create the list widget
    let list = SelectableList::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Resources ({})", resources.len()))
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        );

    // Create state from app state
    let mut list_state = SelectableListState {
        selected: app.state().list_state.selected,
        offset: app.state().list_state.offset,
    };

    // Render the list
    list.render(area, f.buffer_mut(), &mut list_state);

    Ok(())
}

/// Draw resource detail view
fn draw_resource_detail(f: &mut Frame, area: Rect, _app: &TuiApp) -> Result<()> {
    let content = Paragraph::new("Resource Detail view\n\nSelect a resource to view its details.")
        .block(Block::default().borders(Borders::ALL).title("Resource Detail"))
        .style(Style::default().fg(Color::Gray));
    f.render_widget(content, area);
    Ok(())
}

/// Draw logs view
fn draw_logs(f: &mut Frame, area: Rect, _app: &TuiApp) -> Result<()> {
    let content = Paragraph::new("Logs view\n\nSelect a resource to view its logs.")
        .block(Block::default().borders(Borders::ALL).title("Logs"))
        .style(Style::default().fg(Color::Gray));
    f.render_widget(content, area);
    Ok(())
}

/// Draw the footer with keybindings
fn draw_footer(f: &mut Frame, area: Rect, app: &TuiApp) {
    let keybindings = if app.state().command_palette_open {
        vec![
            ("Esc", "Close"),
        ]
    } else {
        vec![
            ("q", "Quit"),
            ("↑↓/jk", "Navigate"),
            ("g/G", "Top/Bottom"),
            ("Enter", "Select"),
            ("r", "Refresh"),
            ("?", "Help"),
        ]
    };

    let spans: Vec<Span> = keybindings
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(*key, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(": "),
                Span::raw(*desc),
                Span::raw("  "),
            ]
        })
        .collect();

    let footer = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL));

    f.render_widget(footer, area);
}

/// Draw error overlay
fn draw_error_overlay(f: &mut Frame, app: &TuiApp) {
    if let Some(error) = &app.state().error {
        let area = f.size();
        
        // Center the error dialog
        let width = area.width.min(60);
        let height = 7;
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        
        let error_area = Rect {
            x,
            y,
            width,
            height,
        };

        // Clear the area
        let clear = Block::default()
            .style(Style::default().bg(Color::Black));
        f.render_widget(clear, error_area);

        // Draw error message
        let error_text = format!("Error\n\n{}\n\nPress Esc to dismiss", error);
        let error_widget = Paragraph::new(error_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red))
                    .title("Error")
            )
            .style(Style::default().fg(Color::Red));
        
        f.render_widget(error_widget, error_area);
    }
}
