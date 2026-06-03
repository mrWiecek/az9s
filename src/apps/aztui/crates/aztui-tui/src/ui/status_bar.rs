use aztui_core::state::{AppState, DataSource};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Status bar component
/// 
/// Displays connection status, context information, and statistics
/// in a compact footer bar, similar to k9s status line.
pub struct StatusBar;

impl StatusBar {
    /// Render the status bar line
    /// 
    /// Format: ● Status | Context | Stats | Indicators
    pub fn render(state: &AppState) -> Line<'static> {
        let mut spans = vec![
            Self::connection_indicator(state),
            Span::raw(" | "),
        ];

        // Current subscription context
        if let Some(sub) = &state.selected_subscription {
            spans.push(Span::styled(
                format!("Sub: {} ", Self::truncate(&sub.name, 20)),
                Style::default().fg(Color::Yellow),
            ));
            spans.push(Span::raw("| "));
        }

        // Item count in current view
        let count = state.current_list_len();
        if count > 0 {
            spans.push(Span::raw(format!("{} items ", count)));
            spans.push(Span::raw("| "));
        }

        // Filter indicator
        if !state.filter.is_empty() {
            spans.push(Span::styled(
                "Filtered ",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw("| "));
        }

        // Loading indicator
        if state.loading {
            spans.push(Span::styled(
                "⟳ Loading",
                Style::default().fg(Color::Green),
            ));
        } else {
            spans.push(Span::styled(
                "Ready",
                Style::default().fg(Color::Green),
            ));
        }

        Line::from(spans)
    }

    /// Render connection status indicator
    /// 
    /// Returns a colored dot with status text:
    /// - Green: Connected (data from Azure)
    /// - Yellow: Cached (data from local cache)
    /// - Red: Disconnected (no data)
    fn connection_indicator(state: &AppState) -> Span<'static> {
        match state.data_source {
            DataSource::Azure => Span::styled(
                "● Azure",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            DataSource::Cache => Span::styled(
                "◐ Cached",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            DataSource::None => Span::styled(
                "○ Disconnected",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
        }
    }

    /// Truncate long strings with ellipsis
    fn truncate(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len.saturating_sub(3)])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aztui_core::models::Subscription;

    #[test]
    fn test_status_bar_basic() {
        let state = AppState::default();
        let line = StatusBar::render(&state);
        
        let text = format!("{:?}", line);
        assert!(text.contains("Disconnected"));
        assert!(text.contains("Ready"));
    }

    #[test]
    fn test_status_bar_with_subscription() {
        let mut state = AppState::default();
        state.selected_subscription = Some(Subscription {
            id: "sub-123".to_string(),
            name: "Production".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-123".to_string(),
        });
        
        let line = StatusBar::render(&state);
        let text = format!("{:?}", line);
        
        assert!(text.contains("Sub:"));
        assert!(text.contains("Production"));
    }

    #[test]
    fn test_status_bar_with_items() {
        let mut state = AppState::default();
        state.subscriptions = vec![
            Subscription {
                id: "sub-1".to_string(),
                name: "Sub 1".to_string(),
                state: "Enabled".to_string(),
                tenant_id: "tenant-1".to_string(),
            },
            Subscription {
                id: "sub-2".to_string(),
                name: "Sub 2".to_string(),
                state: "Enabled".to_string(),
                tenant_id: "tenant-2".to_string(),
            },
        ];
        
        let line = StatusBar::render(&state);
        let text = format!("{:?}", line);
        
        assert!(text.contains("2 items"));
    }

    #[test]
    fn test_status_bar_with_filter() {
        let state = AppState {
            filter: "vm".to_string(),
            ..Default::default()
        };
        
        let line = StatusBar::render(&state);
        let text = format!("{:?}", line);
        
        assert!(text.contains("Filtered"));
    }

    #[test]
    fn test_status_bar_loading() {
        let state = AppState {
            loading: true,
            ..Default::default()
        };
        
        let line = StatusBar::render(&state);
        let text = format!("{:?}", line);
        
        assert!(text.contains("Loading"));
        assert!(text.contains("⟳"));
    }

    #[test]
    fn test_truncate_short_string() {
        let result = StatusBar::truncate("Short", 20);
        assert_eq!(result, "Short");
    }

    #[test]
    fn test_truncate_long_string() {
        let result = StatusBar::truncate("This is a very long subscription name", 15);
        assert_eq!(result, "This is a ve...");
        assert_eq!(result.len(), 15);
    }
    
    #[test]
    fn test_data_source_azure() {
        let mut state = AppState::default();
        state.set_data_source(DataSource::Azure);
        
        let line = StatusBar::render(&state);
        let text = format!("{:?}", line);
        
        assert!(text.contains("Azure"));
    }
    
    #[test]
    fn test_data_source_cache() {
        let mut state = AppState::default();
        state.set_data_source(DataSource::Cache);
        
        let line = StatusBar::render(&state);
        let text = format!("{:?}", line);
        
        assert!(text.contains("Cached"));
    }
}
