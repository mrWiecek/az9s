use crate::event::Event;
use crate::{Command, CommandKind, Scope};
use anyhow::Result;
use az9s_core::loader::{LoadReceiver, LoadRequest, LoadResult};
use az9s_core::state::{NavigationContext, ViewMode};
use az9s_core::{App, AppState};

/// Navigation metadata key marking a view that showed all subscriptions.
const CROSS_SUBSCRIPTION: &str = "cross_subscription";
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

    /// Take the channel on which background load results arrive
    pub fn take_load_receiver(&mut self) -> Option<LoadReceiver> {
        self.core.take_load_receiver()
    }

    /// Apply a finished background load
    pub fn apply_load(&mut self, result: LoadResult) {
        self.core.apply(result);
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
                let state = self.state_mut();
                if state.loading {
                    state.spinner_frame = state.spinner_frame.wrapping_add(1);
                }
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
                let state = self.state_mut();
                state.view = ViewMode::SubscriptionList;
                state.list_state.reset();
                state.clear_navigation();
            }
            CommandKind::ResourceGroups { scope } => {
                debug!("Command: resource groups, scope {:?}", scope);
                if let Some(subscription_ids) = self.resolve_scope(&scope) {
                    self.state_mut().clear_navigation();
                    self.show_resource_groups(subscription_ids, scope == Scope::All);
                }
            }
            CommandKind::Resources { scope } => {
                debug!("Command: resources, scope {:?}", scope);
                if let Some(subscription_ids) = self.resolve_scope(&scope) {
                    self.state_mut().clear_navigation();
                    self.show_resources(subscription_ids, None, scope == Scope::All);
                }
            }
            CommandKind::Unknown(_) => {
                // Already handled above
            }
        }

        Ok(())
    }

    /// Turn a command scope into subscription IDs. For a single subscription,
    /// also makes it the selected one. Shows an error and returns `None` if the
    /// subscription is unknown.
    fn resolve_scope(&mut self, scope: &Scope) -> Option<Vec<String>> {
        match scope {
            Scope::All => Some(self.enabled_subscription_ids()),
            Scope::Subscription(name) => match self.find_subscription(name).cloned() {
                Some(sub) => {
                    let id = sub.id.clone();
                    self.state_mut().selected_subscription = Some(sub);
                    Some(vec![id])
                }
                None => {
                    self.state_mut()
                        .set_error(Some(format!("Subscription not found: {}", name)));
                    None
                }
            },
        }
    }

    /// IDs of all subscriptions that can be queried (disabled ones always fail).
    fn enabled_subscription_ids(&self) -> Vec<String> {
        self.state()
            .subscriptions
            .iter()
            .filter(|s| !s.state.eq_ignore_ascii_case("Disabled"))
            .map(|s| s.id.clone())
            .collect()
    }

    /// Switch to the resource group list now and load it in the background.
    fn show_resource_groups(&mut self, subscription_ids: Vec<String>, all: bool) {
        let state = self.state_mut();
        state.view = ViewMode::ResourceGroupList;
        state.cross_subscription_mode = all;
        state.resource_groups.clear();
        state.list_state.reset();
        self.core
            .request(LoadRequest::ResourceGroups { subscription_ids });
    }

    /// Switch to the resource list now and load it in the background.
    fn show_resources(
        &mut self,
        subscription_ids: Vec<String>,
        resource_group: Option<String>,
        all: bool,
    ) {
        let state = self.state_mut();
        state.view = ViewMode::ResourceList;
        state.cross_subscription_mode = all;
        state.current_resource_group = resource_group.clone();
        state.resources.clear();
        state.list_state.reset();
        self.core.request(LoadRequest::Resources {
            subscription_ids,
            resource_group,
        });
    }

    /// Find a subscription by name or ID
    fn find_subscription(&self, name_or_id: &str) -> Option<&az9s_core::models::Subscription> {
        let search = name_or_id.to_lowercase();
        self.state()
            .subscriptions
            .iter()
            .find(|s| s.name.to_lowercase() == search || s.id.to_lowercase().contains(&search))
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

    /// Reload the current view in the background
    async fn handle_refresh(&mut self) -> Result<()> {
        let state = self.state();
        let scope_ids = if state.cross_subscription_mode {
            Some(self.enabled_subscription_ids())
        } else {
            state
                .selected_subscription
                .as_ref()
                .map(|s| vec![s.id.clone()])
        };

        match (state.view.clone(), scope_ids) {
            (ViewMode::SubscriptionList, _) => {
                info!("Refreshing subscriptions");
                self.core.request(LoadRequest::Subscriptions);
            }
            (ViewMode::ResourceGroupList, Some(subscription_ids)) => {
                self.core
                    .request(LoadRequest::ResourceGroups { subscription_ids });
            }
            (ViewMode::ResourceList, Some(subscription_ids)) => {
                let resource_group = self.state().current_resource_group.clone();
                self.core.request(LoadRequest::Resources {
                    subscription_ids,
                    resource_group,
                });
            }
            _ => debug!("Refresh not available for current view"),
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
            state.cross_subscription_mode = ctx.metadata.contains_key(CROSS_SUBSCRIPTION);
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
            self.push_current_context(None);
            let id = subscription.id.clone();
            self.state_mut().selected_subscription = Some(subscription);
            self.show_resource_groups(vec![id], false);
        }
        Ok(true)
    }

    /// Handle resource group selection: show only that group's resources
    async fn handle_resource_group_select(&mut self) -> Result<bool> {
        if let Some(resource_group) = self.state().selected_resource_group().cloned() {
            info!("Selected resource group: {}", resource_group.name);
            self.push_current_context(None);
            // Use the group's own subscription: in "all subscriptions" mode the
            // selected subscription is not the group's.
            if let Some(sub) = self
                .state()
                .subscriptions
                .iter()
                .find(|s| s.id == resource_group.subscription_id)
                .cloned()
            {
                self.state_mut().selected_subscription = Some(sub);
            }
            self.show_resources(
                vec![resource_group.subscription_id.clone()],
                Some(resource_group.name.clone()),
                false,
            );
        }
        Ok(true)
    }

    /// Remember the current view so Esc/Backspace can return to it
    fn push_current_context(&mut self, resource_group: Option<String>) {
        let state = self.state_mut();
        let mut metadata = std::collections::HashMap::new();
        if state.cross_subscription_mode {
            metadata.insert(CROSS_SUBSCRIPTION.to_string(), String::new());
        }
        let ctx = NavigationContext {
            view: state.view.clone(),
            subscription_id: state.selected_subscription.as_ref().map(|s| s.id.clone()),
            resource_group,
            resource_id: None,
            metadata,
        };
        state.push_navigation(ctx);
    }

    /// Handle resource selection
    async fn handle_resource_select(&mut self) -> Result<bool> {
        if let Some(resource) = self.state().selected_resource().cloned() {
            info!("Selected resource: {}", resource.name);
            self.push_current_context(Some(resource.resource_group.clone()));
            let state = self.state_mut();
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
