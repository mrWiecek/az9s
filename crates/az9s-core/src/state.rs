use crate::models::{Resource, ResourceGroup, Subscription};
use std::collections::HashMap;

/// Application state management
#[derive(Debug, Default)]
pub struct AppState {
    /// Current view mode
    pub view: ViewMode,

    /// Selected subscription
    pub selected_subscription: Option<Subscription>,

    /// Loaded subscriptions
    pub subscriptions: Vec<Subscription>,

    /// Current resource groups list
    pub resource_groups: Vec<ResourceGroup>,

    /// Current resource list
    pub resources: Vec<Resource>,

    /// Selected resource for detail view
    pub selected_resource_detail: Option<Resource>,

    /// Navigation history
    pub navigation_stack: Vec<NavigationContext>,

    /// Command mode state (k9s-like colon commands)
    pub command_mode: bool,

    /// Command input buffer
    pub command_input: String,

    /// Help screen state
    pub help_open: bool,

    /// Filter mode state
    pub filter_mode: bool,

    /// Filter text
    pub filter: String,

    /// List selection state
    pub list_state: ListState,

    /// Loading state
    pub loading: bool,

    /// Error message
    pub error: Option<String>,

    /// Data source indicator (for UX transparency)
    pub data_source: DataSource,

    /// Cross-subscription mode (for "all" scope)
    pub cross_subscription_mode: bool,
}

/// Indicates where the current data came from
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DataSource {
    #[default]
    None,
    /// Data loaded fresh from Azure API
    Azure,
    /// Data loaded from local cache
    Cache,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    SubscriptionList,
    ResourceGroupList,
    ResourceList,
    ResourceDetail,
    Logs,
}

#[derive(Debug, Clone)]
pub struct NavigationContext {
    pub view: ViewMode,
    pub subscription_id: Option<String>,
    pub resource_group: Option<String>,
    pub resource_id: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// List selection state
#[derive(Debug, Clone, Default)]
pub struct ListState {
    /// Currently selected index
    pub selected: usize,
    /// Scroll offset for viewport
    pub offset: usize,
}

impl ListState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Select next item
    pub fn select_next(&mut self, list_len: usize) {
        if list_len == 0 {
            return;
        }
        self.selected = (self.selected + 1).min(list_len - 1);
    }

    /// Select previous item
    pub fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Select first item
    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    /// Select last item
    pub fn select_last(&mut self, list_len: usize) {
        if list_len > 0 {
            self.selected = list_len - 1;
        }
    }

    /// Get the selected index
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Reset selection
    pub fn reset(&mut self) {
        self.selected = 0;
        self.offset = 0;
    }

    /// Update scroll offset based on viewport height
    pub fn update_offset(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }

        // Ensure selected item is visible
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + viewport_height {
            self.offset = self.selected - viewport_height + 1;
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a new navigation context
    pub fn push_navigation(&mut self, context: NavigationContext) {
        self.navigation_stack.push(context);
    }

    /// Pop the last navigation context
    pub fn pop_navigation(&mut self) -> Option<NavigationContext> {
        self.navigation_stack.pop()
    }

    /// Clear all navigation history
    pub fn clear_navigation(&mut self) {
        self.navigation_stack.clear();
    }

    /// Get the current list length based on view
    pub fn current_list_len(&self) -> usize {
        match self.view {
            ViewMode::SubscriptionList => self.filtered_subscriptions().len(),
            ViewMode::ResourceGroupList => self.filtered_resource_groups().len(),
            ViewMode::ResourceList => self.filtered_resources().len(),
            _ => 0,
        }
    }

    /// Get filtered subscriptions based on current filter
    pub fn filtered_subscriptions(&self) -> Vec<&Subscription> {
        if self.filter.is_empty() {
            self.subscriptions.iter().collect()
        } else {
            let filter_lower = self.filter.to_lowercase();
            self.subscriptions
                .iter()
                .filter(|s| {
                    s.name.to_lowercase().contains(&filter_lower)
                        || s.id.to_lowercase().contains(&filter_lower)
                })
                .collect()
        }
    }

    /// Get filtered resource groups based on current filter
    pub fn filtered_resource_groups(&self) -> Vec<&ResourceGroup> {
        if self.filter.is_empty() {
            self.resource_groups.iter().collect()
        } else {
            let filter_lower = self.filter.to_lowercase();
            self.resource_groups
                .iter()
                .filter(|rg| {
                    rg.name.to_lowercase().contains(&filter_lower)
                        || rg.location.to_lowercase().contains(&filter_lower)
                })
                .collect()
        }
    }

    /// Get filtered resources based on current filter
    pub fn filtered_resources(&self) -> Vec<&Resource> {
        if self.filter.is_empty() {
            self.resources.iter().collect()
        } else {
            let filter_lower = self.filter.to_lowercase();
            self.resources
                .iter()
                .filter(|r| {
                    r.name.to_lowercase().contains(&filter_lower)
                        || r.resource_type.to_lowercase().contains(&filter_lower)
                        || r.location.to_lowercase().contains(&filter_lower)
                })
                .collect()
        }
    }

    /// Get the selected subscription in the current view
    pub fn selected_subscription(&self) -> Option<&Subscription> {
        if self.view == ViewMode::SubscriptionList {
            self.filtered_subscriptions()
                .get(self.list_state.selected)
                .copied()
        } else {
            None
        }
    }

    /// Get the selected resource group
    pub fn selected_resource_group(&self) -> Option<&ResourceGroup> {
        if self.view == ViewMode::ResourceGroupList {
            self.filtered_resource_groups()
                .get(self.list_state.selected)
                .copied()
        } else {
            None
        }
    }

    /// Get the selected resource
    pub fn selected_resource(&self) -> Option<&Resource> {
        if self.view == ViewMode::ResourceList {
            self.filtered_resources()
                .get(self.list_state.selected)
                .copied()
        } else {
            None
        }
    }

    /// Set loading state
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    /// Set error message
    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    /// Clear error
    pub fn clear_error(&mut self) {
        self.error = None;
    }

    /// Set data source
    pub fn set_data_source(&mut self, source: DataSource) {
        self.data_source = source;
    }

    /// Toggle help screen
    pub fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
    }

    /// Enter filter mode
    pub fn enter_filter_mode(&mut self) {
        self.filter_mode = true;
        self.filter.clear();
        self.list_state.reset();
    }

    /// Exit filter mode
    pub fn exit_filter_mode(&mut self) {
        self.filter_mode = false;
        self.filter.clear();
        self.list_state.reset();
    }

    /// Add character to filter
    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.list_state.reset();
    }

    /// Remove last character from filter
    pub fn filter_pop(&mut self) {
        self.filter.pop();
        self.list_state.reset();
    }

    /// Clear filter
    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.list_state.reset();
    }

    /// Enter command mode
    pub fn enter_command_mode(&mut self) {
        self.command_mode = true;
        self.command_input.clear();
    }

    /// Exit command mode
    pub fn exit_command_mode(&mut self) {
        self.command_mode = false;
        self.command_input.clear();
    }

    /// Add character to command input
    pub fn command_push(&mut self, c: char) {
        self.command_input.push(c);
    }

    /// Remove last character from command input
    pub fn command_pop(&mut self) {
        self.command_input.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_state_navigation() {
        let mut state = ListState::new();
        assert_eq!(state.selected(), 0);

        // Navigate down
        state.select_next(5);
        assert_eq!(state.selected(), 1);

        state.select_next(5);
        assert_eq!(state.selected(), 2);

        // Navigate up
        state.select_previous();
        assert_eq!(state.selected(), 1);

        // Navigate to first
        state.select_first();
        assert_eq!(state.selected(), 0);

        // Navigate to last
        state.select_last(5);
        assert_eq!(state.selected(), 4);
    }

    #[test]
    fn test_list_state_bounds() {
        let mut state = ListState::new();

        // Can't go below 0
        state.select_previous();
        assert_eq!(state.selected(), 0);

        // Can't go above list length
        state.select_next(3);
        state.select_next(3);
        state.select_next(3);
        state.select_next(3);
        assert_eq!(state.selected(), 2); // Max is list_len - 1
    }

    #[test]
    fn test_list_state_empty_list() {
        let mut state = ListState::new();

        // Operations on empty list should be safe
        state.select_next(0);
        assert_eq!(state.selected(), 0);

        state.select_last(0);
        assert_eq!(state.selected(), 0);
    }

    #[test]
    fn test_list_state_reset() {
        let mut state = ListState::new();
        state.select_next(10);
        state.select_next(10);
        state.offset = 5;

        state.reset();
        assert_eq!(state.selected(), 0);
        assert_eq!(state.offset, 0);
    }

    #[test]
    fn test_list_state_offset_update() {
        let mut state = ListState::new();
        let viewport_height = 10;

        // Selected item below viewport
        state.selected = 5;
        state.offset = 10;
        state.update_offset(viewport_height);
        assert_eq!(state.offset, 5);

        // Selected item above viewport
        state.selected = 15;
        state.offset = 0;
        state.update_offset(viewport_height);
        assert_eq!(state.offset, 6); // 15 - 10 + 1
    }

    #[test]
    fn test_data_source() {
        let mut state = AppState::new();
        assert_eq!(state.data_source, DataSource::None);

        state.set_data_source(DataSource::Azure);
        assert_eq!(state.data_source, DataSource::Azure);

        state.set_data_source(DataSource::Cache);
        assert_eq!(state.data_source, DataSource::Cache);
    }

    #[test]
    fn test_help_toggle() {
        let mut state = AppState::new();
        assert!(!state.help_open);

        state.toggle_help();
        assert!(state.help_open);

        state.toggle_help();
        assert!(!state.help_open);
    }

    #[test]
    fn test_filter_mode() {
        let mut state = AppState::new();
        assert!(!state.filter_mode);
        assert!(state.filter.is_empty());

        state.enter_filter_mode();
        assert!(state.filter_mode);
        assert!(state.filter.is_empty());

        state.filter_push('t');
        state.filter_push('e');
        state.filter_push('s');
        state.filter_push('t');
        assert_eq!(state.filter, "test");

        state.filter_pop();
        assert_eq!(state.filter, "tes");

        state.exit_filter_mode();
        assert!(!state.filter_mode);
        assert!(state.filter.is_empty());
    }

    #[test]
    fn test_command_mode() {
        let mut state = AppState::new();
        assert!(!state.command_mode);
        assert!(state.command_input.is_empty());

        state.enter_command_mode();
        assert!(state.command_mode);
        assert!(state.command_input.is_empty());

        state.command_push('s');
        state.command_push('u');
        state.command_push('b');
        state.command_push('s');
        assert_eq!(state.command_input, "subs");

        state.command_pop();
        assert_eq!(state.command_input, "sub");

        state.exit_command_mode();
        assert!(!state.command_mode);
        assert!(state.command_input.is_empty());
    }
}
