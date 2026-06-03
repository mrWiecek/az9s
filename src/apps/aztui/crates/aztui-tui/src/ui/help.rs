use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Render the help screen as a modal overlay
pub fn render_help(f: &mut Frame, area: Rect) {
    // Create a centered modal
    let modal_area = centered_rect(80, 90, area);
    
    // Clear the background
    f.render_widget(Clear, modal_area);
    
    // Create the help content
    let help_text = vec![
        Line::from(vec![
            Span::styled("aztui Help", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Navigation", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  j/↓         Move down"),
        Line::from("  k/↑         Move up"),
        Line::from("  g           Jump to first item"),
        Line::from("  G           Jump to last item"),
        Line::from("  Ctrl+d      Page down (10 items)"),
        Line::from("  Ctrl+u      Page up (10 items)"),
        Line::from("  PageDown    Page down (10 items)"),
        Line::from("  PageUp      Page up (10 items)"),
        Line::from("  Enter       Select item / drill down"),
        Line::from("  Esc         Go back / close modal"),
        Line::from("  Backspace   Go back"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Actions", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  r           Refresh current view"),
        Line::from("  Ctrl+r      Force refresh from Azure"),
        Line::from("  /           Enter filter mode"),
        Line::from("  :           Enter command mode (k9s-like)"),
        Line::from("  ?           Toggle this help screen"),
        Line::from("  q           Quit application"),
        Line::from("  Ctrl+c      Quit application"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Command Mode", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  :subs                    Go to subscriptions"),
        Line::from("  :rgs [all|<sub>]         Go to resource groups"),
        Line::from("  :res [all|<sub>]         Go to resources"),
        Line::from(""),
        Line::from("  Examples:"),
        Line::from("    :rgs all               All resource groups across subscriptions"),
        Line::from("    :res my-subscription   Resources in specific subscription"),
        Line::from("    :res all               All resources across subscriptions"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Filter Mode", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  Type to filter the current list by name, type, or location"),
        Line::from("  Enter       Apply filter and exit filter mode"),
        Line::from("  Esc         Clear filter and exit filter mode"),
        Line::from("  Backspace   Delete last character"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Views", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  Subscriptions → Resource Groups → Resources → Resource Details"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Data Sources", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  Azure       Fresh data from Azure API"),
        Line::from("  Cache       Cached data (offline mode)"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Tips", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  • Use filters (/) to quickly find resources"),
        Line::from("  • Use commands (:) for quick navigation"),
        Line::from("  • Check the status bar for data source and connection status"),
        Line::from("  • Press 'r' to refresh if data seems stale"),
        Line::from("  • The app works offline using cached data"),
        Line::from("  • Resource types show concise labels (e.g., 'vm', 'nic', 'storage')"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Press Esc or ? to close", Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC)),
        ]),
    ];
    
    let help_paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
        )
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left);
    
    f.render_widget(help_paragraph, modal_area);
}

/// Helper function to create a centered rectangle
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
    use super::*;

    #[test]
    fn test_centered_rect() {
        let area = Rect::new(0, 0, 100, 100);
        let centered = centered_rect(80, 80, area);
        
        // Should be centered
        assert!(centered.x > 0);
        assert!(centered.y > 0);
        assert!(centered.width < area.width);
        assert!(centered.height < area.height);
    }

    #[test]
    fn test_centered_rect_full_size() {
        let area = Rect::new(0, 0, 100, 100);
        let centered = centered_rect(100, 100, area);
        
        // Should be same as original
        assert_eq!(centered.width, area.width);
        assert_eq!(centered.height, area.height);
    }
}
