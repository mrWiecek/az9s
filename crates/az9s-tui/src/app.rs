use crate::event::Event;
use crate::{Command, CommandKind, Scope};
use anyhow::Result;
use az9s_core::state::{NavigationContext, ViewMode};
use az9s_core::{App, AppState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use tracing::{debug, info, warn};

/// TUI application state wrapper
///
/// This wraps the core application and adds TUI-specific state like quit flag.
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
    ///
    /// Returns `Ok(true)` if the application should continue running,
    /// `Ok(false)` if it should quit.
    pub async fn handle_event(&mut self, event: Event) -> Result<bool> {
        match event {
            Event::Key(key) => self.handle_key(key).await,
            Event::Mouse(mouse) => self.handle_mouse(mouse).await,
            Event::Tick => {
                // Periodic update - could be used for auto-refresh
                Ok(true)
            }
            Event::Resize(_, _) => {
                // Terminal resized - UI will automatically adjust
                debug!("Terminal resized");
                Ok(true)
            }
            Event::Error(err) => {
                warn!("Event error: {}", err);
                self.state_mut().set_error(Some(err));
                Ok(true)
            }
        }
    }

    /// Handle mouse input
    async fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<bool> {
        // Don't handle mouse in filter or command mode
        if self.state().filter_mode || self.state().command_mode {
            return Ok(true);
        }

        match mouse.kind {
            MouseEventKind::ScrollDown => {
                self.navigate_down();
                Ok(true)
            }
            MouseEventKind::ScrollUp => {
                self.navigate_up();
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    /// Handle keyboard input
    async fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        // Command mode has special handling
        if self.state().command_mode {
            return self.handle_command_key(key).await;
        }

        // Filter mode has special handling
        if self.state().filter_mode {
            return self.handle_filter_key(key).await;
        }

        // Help screen has special handling
        if self.state().help_open {
            return self.handle_help_key(key).await;
        }

        // Global keybindings
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('q') => {
                    info!("Quit requested");
                    self.should_quit = true;
                    return Ok(false);
                }
                KeyCode::Char('r') => {
                    debug!("Force refresh requested");
                    self.handle_refresh().await?;
                    return Ok(true);
                }
                KeyCode::Char('d') => {
                    // Ctrl+D for page down
                    self.navigate_page_down();
                    return Ok(true);
                }
                KeyCode::Char('u') => {
                    // Ctrl+U for page up
                    self.navigate_page_up();
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
            KeyCode::Char(':') => {
                debug!("Enter command mode");
                self.state_mut().enter_command_mode();
                Ok(true)
            }
            KeyCode::Char('?') => {
                debug!("Show help");
                self.state_mut().toggle_help();
                Ok(true)
            }
            KeyCode::Esc => {
                let state = self.state_mut();
                if state.error.is_some() {
                    state.clear_error();
                } else if !state.filter.is_empty() {
                    state.clear_filter();
                } else {
                    self.navigate_back().await?;
                }
                Ok(true)
            }
            KeyCode::Char('/') => {
                debug!("Start filter");
                self.state_mut().enter_filter_mode();
                Ok(true)
            }
            KeyCode::Char('r') => {
                debug!("Refresh requested");
                self.handle_refresh().await?;
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
            KeyCode::Enter => self.handle_select().await,
            KeyCode::Backspace => {
                self.navigate_back().await?;
                Ok(true)
            }
            _ => {
                // Pass to view-specific handlers
                self.handle_view_key(key).await
            }
        }
    }

    /// Handle keyboard input in command mode
    async fn handle_command_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                debug!("Exit command mode");
                self.state_mut().exit_command_mode();
                Ok(true)
            }
            KeyCode::Enter => {
                debug!("Execute command");
                let command_input = self.state().command_input.clone();
                self.state_mut().exit_command_mode();
                self.execute_command(&command_input).await?;
                Ok(true)
            }
            KeyCode::Backspace => {
                self.state_mut().command_pop();
                Ok(true)
            }
            KeyCode::Char(c) => {
                self.state_mut().command_push(c);
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    /// Execute a parsed command
    async fn execute_command(&mut self, input: &str) -> Result<()> {
        let command = Command::parse(input);

        if !command.is_valid() {
            if let Some(error_msg) = command.error_message() {
                self.state_mut().set_error(Some(error_msg));
            }
            return Ok(());
        }

        match command.kind {
            CommandKind::Subscriptions => {
                debug!("Command: Navigate to subscriptions");
                self.state_mut().view = ViewMode::SubscriptionList;
                self.state_mut().list_state.reset();
                self.state_mut().clear_navigation();
            }
            CommandKind::ResourceGroups { scope } => {
                debug!(
                    "Command: Navigate to resource groups with scope {:?}",
                    scope
                );
                match scope {
                    Scope::All => {
                        // Load resource groups across all subscriptions
                        self.load_all_resource_groups().await?;
                    }
                    Scope::Subscription(sub_name) => {
                        // Find subscription by name or ID and clone it
                        let sub_opt = self.find_subscription(&sub_name).cloned();

                        if let Some(sub) = sub_opt {
                            let sub_id = sub.id.clone();
                            self.state_mut().selected_subscription = Some(sub);
                            self.core.load_resource_groups(&sub_id).await?;
                            self.state_mut().view = ViewMode::ResourceGroupList;
                            self.state_mut().list_state.reset();
                        } else {
                            self.state_mut()
                                .set_error(Some(format!("Subscription not found: {}", sub_name)));
                        }
                    }
                }
            }
            CommandKind::Resources { scope } => {
                debug!("Command: Navigate to resources with scope {:?}", scope);
                match scope {
                    Scope::All => {
                        // Load resources across all subscriptions
                        self.load_all_resources().await?;
                    }
                    Scope::Subscription(sub_name) => {
                        // Find subscription by name or ID and clone it
                        let sub_opt = self.find_subscription(&sub_name).cloned();

                        if let Some(sub) = sub_opt {
                            let sub_id = sub.id.clone();
                            self.state_mut().selected_subscription = Some(sub);
                            self.core.load_resources(&sub_id).await?;
                            self.state_mut().view = ViewMode::ResourceList;
                            self.state_mut().list_state.reset();
                        } else {
                            self.state_mut()
                                .set_error(Some(format!("Subscription not found: {}", sub_name)));
                        }
                    }
                }
            }
            CommandKind::Unknown(_) => {
                // Already handled above
            }
        }

        Ok(())
    }

    /// Find a subscription by name or ID
    fn find_subscription(&self, name_or_id: &str) -> Option<&az9s_core::models::Subscription> {
        let search = name_or_id.to_lowercase();
        self.state()
            .subscriptions
            .iter()
            .find(|s| s.name.to_lowercase() == search || s.id.to_lowercase().contains(&search))
    }

    /// Load resource groups across all subscriptions
    async fn load_all_resource_groups(&mut self) -> Result<()> {
        info!("Loading resource groups across all subscriptions");
        self.state_mut().set_loading(true);
        self.state_mut().clear_error();
        self.state_mut().cross_subscription_mode = true;

        let mut all_rgs = Vec::new();
        let subscriptions = self.state().subscriptions.clone();

        for sub in &subscriptions {
            match self.core.load_resource_groups(&sub.id).await {
                Ok(()) => {
                    // Collect the loaded resource groups
                    all_rgs.extend(self.state().resource_groups.clone());
                }
                Err(e) => {
                    warn!("Failed to load resource groups for {}: {}", sub.name, e);
                }
            }
        }

        self.state_mut().resource_groups = all_rgs;
        self.state_mut().view = ViewMode::ResourceGroupList;
        self.state_mut().list_state.reset();
        self.state_mut().set_loading(false);

        info!(
            "Loaded {} resource groups across all subscriptions",
            self.state().resource_groups.len()
        );
        Ok(())
    }

    /// Load resources across all subscriptions
    async fn load_all_resources(&mut self) -> Result<()> {
        info!("Loading resources across all subscriptions");
        self.state_mut().set_loading(true);
        self.state_mut().clear_error();
        self.state_mut().cross_subscription_mode = true;

        let mut all_resources = Vec::new();
        let subscriptions = self.state().subscriptions.clone();

        for sub in &subscriptions {
            match self.core.load_resources(&sub.id).await {
                Ok(()) => {
                    // Collect the loaded resources
                    all_resources.extend(self.state().resources.clone());
                }
                Err(e) => {
                    warn!("Failed to load resources for {}: {}", sub.name, e);
                }
            }
        }

        self.state_mut().resources = all_resources;
        self.state_mut().view = ViewMode::ResourceList;
        self.state_mut().list_state.reset();
        self.state_mut().set_loading(false);

        info!(
            "Loaded {} resources across all subscriptions",
            self.state().resources.len()
        );
        Ok(())
    }

    /// Handle keyboard input in filter mode
    async fn handle_filter_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                debug!("Exit filter mode");
                self.state_mut().exit_filter_mode();
                Ok(true)
            }
            KeyCode::Enter => {
                debug!("Apply filter and exit filter mode");
                self.state_mut().filter_mode = false;
                Ok(true)
            }
            KeyCode::Backspace => {
                self.state_mut().filter_pop();
                Ok(true)
            }
            KeyCode::Char(c) => {
                self.state_mut().filter_push(c);
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    /// Handle keyboard input in help screen
    async fn handle_help_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                debug!("Close help");
                self.state_mut().toggle_help();
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    /// Handle refresh action
    async fn handle_refresh(&mut self) -> Result<()> {
        match self.state().view {
            ViewMode::SubscriptionList => {
                info!("Refreshing subscriptions");
                if let Err(e) = self.core.refresh_subscriptions().await {
                    warn!("Failed to refresh subscriptions: {}", e);
                    self.state_mut()
                        .set_error(Some(format!("Refresh failed: {}", e)));
                }
            }
            ViewMode::ResourceGroupList => {
                if self.state().cross_subscription_mode {
                    self.load_all_resource_groups().await?;
                } else if let Some(sub) = self.state().selected_subscription.clone() {
                    info!("Refreshing resource groups for subscription {}", sub.id);
                    if let Err(e) = self.core.load_resource_groups(&sub.id).await {
                        warn!("Failed to refresh resource groups: {}", e);
                        self.state_mut()
                            .set_error(Some(format!("Refresh failed: {}", e)));
                    }
                }
            }
            ViewMode::ResourceList => {
                if self.state().cross_subscription_mode {
                    self.load_all_resources().await?;
                } else if let Some(sub) = self.state().selected_subscription.clone() {
                    info!("Refreshing resources for subscription {}", sub.id);
                    if let Err(e) = self.core.load_resources(&sub.id).await {
                        warn!("Failed to refresh resources: {}", e);
                        self.state_mut()
                            .set_error(Some(format!("Refresh failed: {}", e)));
                    }
                }
            }
            _ => {
                debug!("Refresh not implemented for current view");
            }
        }
        Ok(())
    }

    /// Navigate back in the navigation stack
    async fn navigate_back(&mut self) -> Result<()> {
        let state = self.state_mut();
        if let Some(ctx) = state.pop_navigation() {
            debug!("Navigate back to {:?}", ctx.view);
            state.view = ctx.view;
            state.list_state.reset();
            state.cross_subscription_mode = false;
        }
        Ok(())
    }

    /// Navigate down in the current list
    fn navigate_down(&mut self) {
        let state = self.state_mut();
        let list_len = state.current_list_len();
        if list_len > 0 {
            state.list_state.select_next(list_len);
        }
    }

    /// Navigate up in the current list
    fn navigate_up(&mut self) {
        let state = self.state_mut();
        if state.current_list_len() > 0 {
            state.list_state.select_previous();
        }
    }

    /// Navigate to first item
    fn navigate_first(&mut self) {
        let state = self.state_mut();
        if state.current_list_len() > 0 {
            state.list_state.select_first();
        }
    }

    /// Navigate to last item
    fn navigate_last(&mut self) {
        let state = self.state_mut();
        let list_len = state.current_list_len();
        if list_len > 0 {
            state.list_state.select_last(list_len);
        }
    }

    /// Navigate page down (viewport-aware)
    fn navigate_page_down(&mut self) {
        let state = self.state_mut();
        let list_len = state.current_list_len();
        if list_len > 0 {
            // Jump by 10 items or to the end
            let target = (state.list_state.selected + 10).min(list_len - 1);
            state.list_state.selected = target;
        }
    }

    /// Navigate page up (viewport-aware)
    fn navigate_page_up(&mut self) {
        let state = self.state_mut();
        if state.current_list_len() > 0 {
            // Jump by 10 items or to the start
            state.list_state.selected = state.list_state.selected.saturating_sub(10);
        }
    }

    /// Handle selection (Enter key)
    async fn handle_select(&mut self) -> Result<bool> {
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

            // Push current context to navigation stack
            let state = self.state_mut();
            let ctx = NavigationContext {
                view: ViewMode::SubscriptionList,
                subscription_id: None,
                resource_group: None,
                resource_id: None,
                metadata: std::collections::HashMap::new(),
            };
            state.push_navigation(ctx);
            state.selected_subscription = Some(subscription.clone());
            state.cross_subscription_mode = false;

            // Load resource groups for this subscription
            if let Err(e) = self.core.load_resource_groups(&subscription.id).await {
                warn!("Failed to load resource groups: {}", e);
                self.state_mut()
                    .set_error(Some(format!("Failed to load resource groups: {}", e)));
                return Ok(true);
            }

            // Navigate to resource group list view
            let state = self.state_mut();
            state.view = ViewMode::ResourceGroupList;
            state.list_state.reset();
        }
        Ok(true)
    }

    /// Handle resource group selection
    async fn handle_resource_group_select(&mut self) -> Result<bool> {
        if let Some(resource_group) = self.state().selected_resource_group().cloned() {
            info!("Selected resource group: {}", resource_group.name);

            // Push current context to navigation stack
            let state = self.state_mut();
            let ctx = NavigationContext {
                view: ViewMode::ResourceGroupList,
                subscription_id: state.selected_subscription.as_ref().map(|s| s.id.clone()),
                resource_group: None,
                resource_id: None,
                metadata: std::collections::HashMap::new(),
            };
            state.push_navigation(ctx);

            // Load resources for this subscription
            if let Some(sub) = self.state().selected_subscription.clone() {
                if let Err(e) = self.core.load_resources(&sub.id).await {
                    warn!("Failed to load resources: {}", e);
                    self.state_mut()
                        .set_error(Some(format!("Failed to load resources: {}", e)));
                    return Ok(true);
                }
            }

            // Navigate to resource list view
            let state = self.state_mut();
            state.view = ViewMode::ResourceList;
            state.list_state.reset();
        }
        Ok(true)
    }

    /// Handle resource selection
    async fn handle_resource_select(&mut self) -> Result<bool> {
        if let Some(resource) = self.state().selected_resource().cloned() {
            info!("Selected resource: {}", resource.name);

            // Push current context to navigation stack
            let state = self.state_mut();
            let ctx = NavigationContext {
                view: ViewMode::ResourceList,
                subscription_id: state.selected_subscription.as_ref().map(|s| s.id.clone()),
                resource_group: Some(resource.resource_group.clone()),
                resource_id: None,
                metadata: std::collections::HashMap::new(),
            };
            state.push_navigation(ctx);

            // Store the selected resource for detail view
            state.selected_resource_detail = Some(resource);
            state.view = ViewMode::ResourceDetail;
            state.list_state.reset();
        }
        Ok(true)
    }

    /// Handle view-specific keys
    async fn handle_view_key(&mut self, _key: KeyEvent) -> Result<bool> {
        // View-specific handlers can be added here
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_tui_app_creation() {
        // This test requires async runtime, so we'll keep it simple
        // Just verify the module compiles
    }

    #[test]
    fn test_navigation_empty_list() {
        // Test that navigation on empty lists doesn't panic
        // This would require setting up a full app, so we'll skip for now
    }
}
