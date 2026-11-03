use az9s_core::state::{AppState, ViewMode};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Breadcrumb navigation component
///
/// Renders a breadcrumb trail showing the current navigation path,
/// similar to k9s context display.
pub struct Breadcrumb;

impl Breadcrumb {
    /// Render the breadcrumb navigation line
    ///
    /// Format: az9s > [Subscription] > [View] [| Filter: xxx]
    pub fn render(state: &AppState) -> Line<'static> {
        let mut spans = vec![Span::styled(
            "az9s",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )];

        // The tenant, once one is chosen with :ctx
        if let Some(tenant) = state.tenant_label() {
            spans.push(Span::raw(" > "));
            spans.push(Span::styled(
                format!("[{}]", Self::truncate(&tenant, 30)),
                Style::default().fg(Color::Magenta),
            ));
        }

        // Add the scope: a management group (cross-subscription lists opened
        // from the tree) or the selected subscription.
        let group = state
            .scope_group
            .as_deref()
            .filter(|_| state.cross_subscription_mode)
            .and_then(|g| state.scope_tree.as_ref()?.label_of(g));
        if let Some(group) = group {
            spans.push(Span::raw(" > "));
            spans.push(Span::styled(
                Self::truncate(group, 30),
                Style::default().fg(Color::Yellow),
            ));
        } else if let Some(sub) = &state.selected_subscription {
            spans.push(Span::raw(" > "));
            spans.push(Span::styled(
                Self::truncate(&sub.name, 30),
                Style::default().fg(Color::Yellow),
            ));
        }

        // Add current view
        spans.push(Span::raw(" > "));
        spans.push(Span::styled(
            Self::view_name(&state.view),
            Style::default().fg(Color::White),
        ));

        // Add filter indicator if active
        if !state.filter.is_empty() {
            spans.push(Span::raw(" | "));
            spans.push(Span::styled(
                format!("Filter: {}", state.filter),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::ITALIC),
            ));
        }

        // Add loading indicator
        if state.loading {
            spans.push(Span::raw("  "));
            spans.push(Span::styled("⟳", Style::default().fg(Color::Green)));
        }

        Line::from(spans)
    }

    /// Get human-readable view name
    fn view_name(view: &ViewMode) -> &'static str {
        match view {
            ViewMode::ManagementGroupTree => "Management Groups",
            ViewMode::SubscriptionList => "Subscriptions",
            ViewMode::ResourceGroupList => "Resource Groups",
            ViewMode::ResourceList => "Resources",
            ViewMode::ResourceTypeSummary => "Resource Types",
            ViewMode::TagSummary => "Tags",
            ViewMode::TenantList => "Tenants",
            ViewMode::ResourceDetail => "Resource Detail",
            ViewMode::Logs => "Logs",
        }
    }

    /// Truncate long strings with ellipsis
    ///
    /// This ensures breadcrumbs don't overflow on narrow terminals
    fn truncate(s: &str, max_len: usize) -> String {
        // Character-based: byte slicing panics on non-ASCII names.
        super::widgets::truncate_with_ellipsis(s, max_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use az9s_core::models::Subscription;

    #[test]
    fn test_breadcrumb_basic() {
        let state = AppState::default();
        let line = Breadcrumb::render(&state);

        // Should contain az9s and Subscriptions
        let text = format!("{:?}", line);
        assert!(text.contains("az9s"));
        assert!(text.contains("Subscriptions"));
    }

    #[test]
    fn test_breadcrumb_with_subscription() {
        let mut state = AppState::default();
        state.selected_subscription = Some(Subscription {
            id: "sub-123".to_string(),
            name: "Production".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-123".to_string(),
        });

        let line = Breadcrumb::render(&state);
        let text = format!("{:?}", line);

        assert!(text.contains("Production"));
    }

    #[test]
    fn test_breadcrumb_truncation() {
        let mut state = AppState::default();
        state.selected_subscription = Some(Subscription {
            id: "sub-123".to_string(),
            name: "Very Long Subscription Name That Exceeds The Maximum Length Allowed".to_string(),
            state: "Enabled".to_string(),
            tenant_id: "tenant-123".to_string(),
        });

        let line = Breadcrumb::render(&state);
        let text = format!("{:?}", line);

        // Should be truncated with ellipsis
        assert!(text.contains("..."));
    }

    #[test]
    fn test_breadcrumb_with_filter() {
        let mut state = AppState::default();
        state.filter = "vm".to_string();

        let line = Breadcrumb::render(&state);
        let text = format!("{:?}", line);

        assert!(text.contains("Filter"));
        assert!(text.contains("vm"));
    }

    #[test]
    fn test_breadcrumb_loading() {
        let mut state = AppState::default();
        state.loading = true;

        let line = Breadcrumb::render(&state);
        let text = format!("{:?}", line);

        assert!(text.contains("⟳"));
    }

    #[test]
    fn test_truncate_non_ascii_does_not_panic() {
        let result = Breadcrumb::truncate("Zarządzanie-grupą-główną-ąęółżź", 10);
        assert_eq!(result.chars().count(), 10);
    }

    #[test]
    fn test_truncate_short_string() {
        let result = Breadcrumb::truncate("Short", 20);
        assert_eq!(result, "Short");
    }

    #[test]
    fn test_truncate_long_string() {
        let result = Breadcrumb::truncate("This is a very long string", 10);
        assert_eq!(result, "This is...");
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn test_view_names() {
        assert_eq!(
            Breadcrumb::view_name(&ViewMode::SubscriptionList),
            "Subscriptions"
        );
        assert_eq!(Breadcrumb::view_name(&ViewMode::ResourceList), "Resources");
        assert_eq!(
            Breadcrumb::view_name(&ViewMode::ResourceGroupList),
            "Resource Groups"
        );
        assert_eq!(
            Breadcrumb::view_name(&ViewMode::ResourceDetail),
            "Resource Detail"
        );
        assert_eq!(Breadcrumb::view_name(&ViewMode::Logs), "Logs");
    }
}
