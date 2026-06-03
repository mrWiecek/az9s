use crate::models::{Resource, Subscription};
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
    
    /// Current resource list
    pub resources: Vec<Resource>,
    
    /// Navigation history
    pub navigation_stack: Vec<NavigationContext>,
    
    /// Command palette state
    pub command_palette_open: bool,
    
    /// Filter text
    pub filter: String,
    
    /// List selection state
    pub list_state: ListState,
    
    /// Loading state
    pub loading: bool,
    
    /// Error message
    pub error: Option<String>,
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
            ViewMode::SubscriptionList => self.subscriptions.len(),
            ViewMode::ResourceList => self.resources.len(),
            _ => 0,
        }
    }

    /// Get the selected item in the current view
    pub fn selected_subscription(&self) -> Option<&Subscription> {
        if self.view == ViewMode::SubscriptionList {
            self.subscriptions.get(self.list_state.selected)
        } else {
            None
        }
    }

    /// Get the selected resource
    pub fn selected_resource(&self) -> Option<&Resource> {
        if self.view == ViewMode::ResourceList {
            self.resources.get(self.list_state.selected)
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
}
