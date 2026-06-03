use crate::event::Event;
use anyhow::Result;
use aztui_core::{App, AppState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tracing::{debug, info};

/// TUI application state wrapper
pub struct TuiApp {
    core: App,
    should_quit: bool,
}

impl TuiApp {
    /// Create a new TUI application
    pub fn new(core: App) -> Self {
        Self {
            core,
            should_quit: false,
        }
    }

    /// Get the core application
    pub fn core(&self) -> &App {
        &self.core
    }

    /// Get mutable core application
    pub fn core_mut(&mut self) -> &mut App {
        &mut self.core
    }

    /// Get the application state
    pub fn state(&self) -> &AppState {
        self.core.state()
    }

    /// Get mutable application state
    pub fn state_mut(&mut self) -> &mut AppState {
        self.core.state_mut()
    }

    /// Check if the application should quit
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Handle an event
    pub async fn handle_event(&mut self, event: Event) -> Result<bool> {
        match event {
            Event::Key(key) => self.handle_key(key).await,
            Event::Tick => {
                // Periodic update - could be used for auto-refresh
                Ok(true)
            }
            Event::Resize(_, _) => {
                // Terminal resized - UI will automatically adjust
                Ok(true)
            }
        }
    }

    /// Handle keyboard input
    async fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        // Global keybindings
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('q') => {
                    info!("Quit requested");
                    self.should_quit = true;
                    return Ok(false);
                }
                KeyCode::Char('p') => {
                    debug!("Toggle command palette");
                    let state = self.state_mut();
                    state.command_palette_open = !state.command_palette_open;
                    return Ok(true);
                }
                _ => {}
            }
        }

        // View-specific keybindings
        match key.code {
            KeyCode::Char('q') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                info!("Quit requested");
                self.should_quit = true;
                Ok(false)
            }
            KeyCode::Char('?') => {
                debug!("Show help");
                // TODO: Implement help view
                Ok(true)
            }
            KeyCode::Esc => {
                let state = self.state_mut();
                if state.command_palette_open {
                    state.command_palette_open = false;
                } else if state.error.is_some() {
                    state.clear_error();
                } else if let Some(_ctx) = state.pop_navigation() {
                    debug!("Navigate back");
                    // Reset list state when navigating back
                    state.list_state.reset();
                }
                Ok(true)
            }
            KeyCode::Char('/') => {
                debug!("Start filter");
                // TODO: Implement filter mode
                Ok(true)
            }
            KeyCode::Char('r') => {
                debug!("Refresh requested");
                // TODO: Implement refresh
                Ok(true)
            }
            // Navigation keys
            KeyCode::Down | KeyCode::Char('j') => {
                self.navigate_down();
                Ok(true)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.navigate_up();
                Ok(true)
            }
            KeyCode::Char('g') => {
                self.navigate_first();
                Ok(true)
            }
            KeyCode::Char('G') => {
                self.navigate_last();
                Ok(true)
            }
            KeyCode::PageDown => {
                self.navigate_page_down();
                Ok(true)
            }
            KeyCode::PageUp => {
                self.navigate_page_up();
                Ok(true)
            }
            KeyCode::Enter => {
                self.handle_select().await
            }
            _ => {
                // Pass to view-specific handlers
                self.handle_view_key(key).await
            }
        }
    }

    /// Navigate down in the current list
    fn navigate_down(&mut self) {
        let state = self.state_mut();
        let list_len = state.current_list_len();
        state.list_state.select_next(list_len);
    }

    /// Navigate up in the current list
    fn navigate_up(&mut self) {
        let state = self.state_mut();
        state.list_state.select_previous();
    }

    /// Navigate to first item
    fn navigate_first(&mut self) {
        let state = self.state_mut();
        state.list_state.select_first();
    }

    /// Navigate to last item
    fn navigate_last(&mut self) {
        let state = self.state_mut();
        let list_len = state.current_list_len();
        state.list_state.select_last(list_len);
    }

    /// Navigate page down
    fn navigate_page_down(&mut self) {
        let state = self.state_mut();
        let list_len = state.current_list_len();
        // Move down by 10 items or to the end
        for _ in 0..10 {
            state.list_state.select_next(list_len);
        }
    }

    /// Navigate page up
    fn navigate_page_up(&mut self) {
        let state = self.state_mut();
        // Move up by 10 items or to the start
        for _ in 0..10 {
            state.list_state.select_previous();
        }
    }

    /// Handle selection (Enter key)
    async fn handle_select(&mut self) -> Result<bool> {
        use aztui_core::state::ViewMode;

        match self.state().view {
            ViewMode::SubscriptionList => self.handle_subscription_select().await,
            ViewMode::ResourceGroupList => self.handle_resource_group_select().await,
            ViewMode::ResourceList => self.handle_resource_select().await,
            ViewMode::ResourceDetail => Ok(true),
            ViewMode::Logs => Ok(true),
        }
    }

    /// Handle subscription selection
    async fn handle_subscription_select(&mut self) -> Result<bool> {
        if let Some(subscription) = self.state().selected_subscription().cloned() {
            info!("Selected subscription: {}", subscription.name);
            
            // TODO: Navigate to resource groups view
            // For now, just log the selection
            let state = self.state_mut();
            state.selected_subscription = Some(subscription);
        }
        Ok(true)
    }

    /// Handle resource group selection
    async fn handle_resource_group_select(&mut self) -> Result<bool> {
        debug!("Resource group selected");
        // TODO: Navigate to resources in this group
        Ok(true)
    }

    /// Handle resource selection
    async fn handle_resource_select(&mut self) -> Result<bool> {
        if let Some(resource) = self.state().selected_resource() {
            info!("Selected resource: {}", resource.name);
            // TODO: Navigate to resource detail view
        }
        Ok(true)
    }

    /// Handle view-specific keyboard input
    async fn handle_view_key(&mut self, key: KeyEvent) -> Result<bool> {
        use aztui_core::state::ViewMode;

        match self.state().view {
            ViewMode::SubscriptionList => self.handle_subscription_list_key(key).await,
            ViewMode::ResourceGroupList => self.handle_resource_group_list_key(key).await,
            ViewMode::ResourceList => self.handle_resource_list_key(key).await,
            ViewMode::ResourceDetail => self.handle_resource_detail_key(key).await,
            ViewMode::Logs => self.handle_logs_key(key).await,
        }
    }

    async fn handle_subscription_list_key(&mut self, _key: KeyEvent) -> Result<bool> {
        // View-specific keys handled in main handler
        Ok(true)
    }

    async fn handle_resource_group_list_key(&mut self, _key: KeyEvent) -> Result<bool> {
        Ok(true)
    }

    async fn handle_resource_list_key(&mut self, _key: KeyEvent) -> Result<bool> {
        Ok(true)
    }

    async fn handle_resource_detail_key(&mut self, _key: KeyEvent) -> Result<bool> {
        Ok(true)
    }

    async fn handle_logs_key(&mut self, _key: KeyEvent) -> Result<bool> {
        Ok(true)
    }
}
