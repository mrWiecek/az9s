use crate::desktop::{Desktop, SystemDesktop};
use crate::event::Event;
use crate::ui::search;
use crate::{Command, CommandKind, Scope};
use anyhow::Result;
use az9s_core::complete;
use az9s_core::detail;
use az9s_core::loader::{LoadReceiver, LoadRequest, LoadResult};
use az9s_core::palette::PaletteAction;
use az9s_core::policy::PolicyScope;
use az9s_core::query::ResourceQuery;
use az9s_core::scope_tree::TreeRowKind;
use az9s_core::state::{DetailTab, NavigationContext, ViewMode};
use az9s_core::{App, AppState};
use std::time::{Duration, Instant};

/// Navigation metadata key marking a view that showed all subscriptions.
const CROSS_SUBSCRIPTION: &str = "cross_subscription";
/// Navigation metadata key: the management group a view was scoped to.
const SCOPE_GROUP: &str = "scope_group";
/// Navigation metadata key: the structured resource filter, as typed.
const QUERY: &str = "query";
/// Navigation metadata key: the row that was selected, restored on going back.
const SELECTED: &str = "selected";
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use tracing::{debug, info, warn};

/// TUI application state wrapper
///
/// This wraps the core application and adds TUI-specific state like quit flag.
pub struct TuiApp {
    core: App,
    should_quit: bool,
    desktop: Box<dyn Desktop>,
    /// When the current view was last loaded by auto-refresh or `r`
    last_refresh: Instant,
}

impl TuiApp {
    /// Create a new TUI application
    pub fn new(core: App) -> Self {
        Self::with_desktop(core, Box::new(SystemDesktop))
    }

    /// Like [`TuiApp::new`], with a custom clipboard/browser (for tests).
    pub fn with_desktop(core: App, desktop: Box<dyn Desktop>) -> Self {
        Self {
            core,
            should_quit: false,
            desktop,
            last_refresh: Instant::now(),
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
                self.tick_at(Instant::now()).await?;
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

        // The command palette takes every key while open
        if self.state().palette.is_some() {
            return self.handle_palette_key(key).await;
        }

        // Typing a search in the detail view (after `/`)
        if self.state().detail_search_input.is_some() {
            self.handle_search_key(key);
            return Ok(true);
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
                KeyCode::Char('p') => {
                    debug!("Open the command palette");
                    let palette = az9s_core::palette::Palette::open(self.state());
                    self.state_mut().palette = Some(palette);
                    return Ok(true);
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
                } else if state.view == ViewMode::ResourceDetail && !state.detail_search.is_empty()
                {
                    // First Esc clears the search highlights, the next goes back.
                    state.clear_detail_search();
                    state.notice = None;
                } else {
                    self.navigate_back().await?;
                }
                Ok(true)
            }
            KeyCode::Char('/') if self.state().view == ViewMode::ResourceDetail => {
                debug!("Start search");
                let state = self.state_mut();
                state.detail_search_origin = (state.detail_scroll, state.detail_match);
                state.detail_search_input = Some(String::new());
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
            // Autocomplete: Tab takes the highlighted suggestion; ↑/↓ pick.
            KeyCode::Tab => {
                let state = self.state();
                let completion = complete::complete(&state.command_input, state);
                if let Some(s) = completion
                    .items
                    .get(state.command_selection)
                    .or(completion.items.first())
                {
                    let input = complete::apply(&state.command_input, completion.start, &s.text);
                    let state = self.state_mut();
                    state.command_input = input;
                    state.command_selection = 0;
                }
                Ok(true)
            }
            KeyCode::Down | KeyCode::Up => {
                let state = self.state();
                let n = complete::complete(&state.command_input, state).items.len();
                if n > 0 {
                    let step = if key.code == KeyCode::Down { 1 } else { n - 1 };
                    let state = self.state_mut();
                    state.command_selection = (state.command_selection + step) % n;
                }
                Ok(true)
            }
            KeyCode::Char(c) => {
                self.state_mut().command_push(c);
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    /// Keys while the palette is open: type to search, ↑/↓ (or Ctrl+P/N)
    /// to choose, Enter to go, Esc to close.
    async fn handle_palette_key(&mut self, key: KeyEvent) -> Result<bool> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let Some(palette) = self.state_mut().palette.as_mut() else {
            return Ok(true);
        };
        match key.code {
            KeyCode::Esc => self.state_mut().palette = None,
            KeyCode::Enter => {
                let action = palette.chosen().map(|r| r.item.action.clone());
                self.state_mut().palette = None;
                if let Some(action) = action {
                    self.run_palette_action(action).await?;
                }
            }
            KeyCode::Down => palette.select(1),
            KeyCode::Up => palette.select(-1),
            KeyCode::Char('n') if ctrl => palette.select(1),
            KeyCode::Char('p') if ctrl => palette.select(-1),
            KeyCode::Char('c') if ctrl => {
                self.should_quit = true;
                return Ok(false);
            }
            KeyCode::Backspace => {
                let mut query = palette.query.clone();
                query.pop();
                palette.set_query(query);
            }
            KeyCode::Char(c) if !ctrl => {
                let query = format!("{}{c}", palette.query);
                palette.set_query(query);
            }
            _ => {}
        }
        Ok(true)
    }

    /// Go where a palette item points. Esc afterwards returns to where the
    /// palette was opened.
    async fn run_palette_action(&mut self, action: PaletteAction) -> Result<()> {
        match action {
            PaletteAction::Command(command) => self.execute_command(&command).await?,
            PaletteAction::Help => self.state_mut().toggle_help(),
            PaletteAction::Subscription(id) => {
                self.push_current_context(None);
                let sub = self
                    .state()
                    .subscriptions
                    .iter()
                    .find(|s| s.id == id)
                    .cloned();
                let state = self.state_mut();
                state.selected_subscription = sub;
                state.scope_group = None;
                self.show_resource_groups(vec![id], false);
            }
            PaletteAction::ManagementGroup(group) => {
                let ids = self.enabled_subscriptions_under(Some(&group));
                if ids.is_empty() {
                    self.state_mut().notice =
                        Some("No enabled subscriptions under that management group".into());
                    return Ok(());
                }
                self.push_current_context(None);
                let state = self.state_mut();
                state.scope_group = Some(group);
                state.selected_subscription = None;
                self.show_resource_groups(ids, true);
            }
            PaletteAction::ResourceGroup {
                subscription_id,
                name,
            } => {
                self.push_current_context(None);
                let sub = self
                    .state()
                    .subscriptions
                    .iter()
                    .find(|s| s.id.eq_ignore_ascii_case(&subscription_id))
                    .cloned();
                let state = self.state_mut();
                state.selected_subscription = sub;
                state.scope_group = None;
                self.show_resources(vec![subscription_id], Some(name), false);
            }
            PaletteAction::Resource(id) => {
                let resource = self.state().resources.iter().find(|r| r.id == id).cloned();
                if let Some(resource) = resource {
                    self.push_current_context(Some(resource.resource_group.clone()));
                    self.core.open_resource_detail(resource);
                }
            }
        }
        Ok(())
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

        // A command without a scope means "where I am": decide that before
        // the command resets the navigation.
        let context = self.current_context();
        // Commands always start a new, unscoped navigation.
        self.state_mut().scope_group = None;
        let mut command = command;
        match &mut command.kind {
            CommandKind::ResourceGroups { scope, .. }
            | CommandKind::Resources { scope, .. }
            | CommandKind::ResourceTypes { scope, .. }
            | CommandKind::Tags { scope, .. }
            | CommandKind::Policy { scope }
                if *scope == Scope::Current =>
            {
                *scope = context;
            }
            _ => {}
        }
        match command.kind {
            CommandKind::Policy { scope } => {
                debug!("Command: policy, scope {:?}", scope);
                if let Some(policy_scope) = self.policy_scope(&scope) {
                    self.state_mut().clear_navigation();
                    self.core.open_policy(policy_scope);
                }
            }
            CommandKind::Context(None) => {
                debug!("Command: tenants");
                let state = self.state_mut();
                state.clear_navigation();
                state.view = ViewMode::TenantList;
                state.list_state.reset();
                self.core.request(LoadRequest::Tenants);
            }
            CommandKind::Context(Some(arg)) => {
                if arg.eq_ignore_ascii_case("all") {
                    self.core.switch_tenant(None);
                } else {
                    match self.find_tenant(&arg) {
                        Some(id) => self.core.switch_tenant(Some(id)),
                        None => self.state_mut().set_error(Some(format!(
                            "Tenant not found: {arg}\n\nType :ctx to list the tenants you can use."
                        ))),
                    }
                }
            }
            CommandKind::ManagementGroups => {
                debug!("Command: management group tree");
                self.state_mut().clear_navigation();
                self.core.open_scope_tree();
            }
            CommandKind::Subscriptions => {
                debug!("Command: Navigate to subscriptions");
                let state = self.state_mut();
                state.view = ViewMode::SubscriptionList;
                state.list_state.reset();
                state.clear_navigation();
            }
            CommandKind::ResourceGroups { scope, query } => {
                debug!("Command: resource groups, scope {:?}", scope);
                if let Some(subscription_ids) = self.resolve_scope(&scope) {
                    self.state_mut().clear_navigation();
                    self.show_resource_groups(subscription_ids, scope.is_cross_subscription());
                    self.state_mut().resource_query = query;
                }
            }
            CommandKind::Resources { scope, query } => {
                debug!("Command: resources, scope {:?}, filter {}", scope, query);
                if let Some(subscription_ids) = self.resolve_scope(&scope) {
                    self.state_mut().clear_navigation();
                    self.show_resources(subscription_ids, None, scope.is_cross_subscription());
                    self.state_mut().resource_query = query;
                }
            }
            CommandKind::ResourceTypes { scope, query } => {
                debug!(
                    "Command: resource types, scope {:?}, filter {}",
                    scope, query
                );
                if let Some(subscription_ids) = self.resolve_scope(&scope) {
                    self.state_mut().clear_navigation();
                    self.show_resources(subscription_ids, None, scope.is_cross_subscription());
                    let state = self.state_mut();
                    state.resource_query = query;
                    state.view = ViewMode::ResourceTypeSummary;
                }
            }
            CommandKind::Tags { scope, query } => {
                debug!("Command: tags, scope {:?}, filter {}", scope, query);
                if let Some(subscription_ids) = self.resolve_scope(&scope) {
                    self.state_mut().clear_navigation();
                    self.show_resources(subscription_ids, None, scope.is_cross_subscription());
                    let state = self.state_mut();
                    state.resource_query = query;
                    state.view = ViewMode::TagSummary;
                }
            }
            CommandKind::Invalid(_) => {
                // Already handled above
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
            Scope::All | Scope::Current => Some(self.enabled_subscription_ids()),
            Scope::Group(group) => {
                let ids = self.enabled_subscriptions_under(Some(group));
                if ids.is_empty() {
                    self.state_mut()
                        .set_error(Some(format!("No enabled subscriptions under {group}")));
                    return None;
                }
                let state = self.state_mut();
                state.scope_group = Some(group.clone());
                state.selected_subscription = None;
                Some(ids)
            }
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

    /// The policy scope for a command scope: one subscription, a management
    /// group, or (for `all`) the tenant root group. Shows an error and
    /// returns `None` when that can't be determined.
    fn policy_scope(&mut self, scope: &Scope) -> Option<PolicyScope> {
        match scope {
            Scope::Group(group) => Some(PolicyScope::Group(group.clone())),
            Scope::Subscription(name) => match self.find_subscription(name).cloned() {
                Some(sub) => {
                    let id = sub.id.clone();
                    self.state_mut().selected_subscription = Some(sub);
                    Some(PolicyScope::Subscription(id))
                }
                None => {
                    self.state_mut()
                        .set_error(Some(format!("Subscription not found: {name}")));
                    None
                }
            },
            Scope::All | Scope::Current => match &self.state().scope_tree {
                Some(tree) => Some(PolicyScope::Group(tree.root.clone())),
                None => {
                    self.state_mut().set_error(Some(
                        "Policy for all subscriptions needs the management group tree, \
                         which isn't loaded (see :mg). Name a subscription instead: \
                         :policy <subscription>"
                            .into(),
                    ));
                    None
                }
            },
        }
    }

    /// What a command without a scope applies to: the highlighted
    /// subscription or management group in the list/tree, else the
    /// management group or subscription the current view is scoped to, else
    /// all subscriptions.
    fn current_context(&self) -> Scope {
        let state = self.state();
        match state.view {
            ViewMode::SubscriptionList => {
                if let Some(sub) = state.selected_subscription() {
                    return Scope::Subscription(sub.id.clone());
                }
            }
            ViewMode::ManagementGroupTree => {
                if let Some(row) = state.selected_tree_row() {
                    return match row.kind {
                        TreeRowKind::Subscription => Scope::Subscription(row.key),
                        TreeRowKind::ManagementGroup => Scope::Group(row.key),
                        TreeRowKind::Unplaced => Scope::All,
                    };
                }
            }
            _ => {}
        }
        if state.cross_subscription_mode {
            return match &state.scope_group {
                Some(group) => Scope::Group(group.clone()),
                None => Scope::All,
            };
        }
        match (
            &state.selected_subscription,
            &state.config.default_subscription,
        ) {
            (Some(sub), _) => Scope::Subscription(sub.id.clone()),
            // No context: the config's default subscription, if any.
            (None, Some(name)) => Scope::Subscription(name.clone()),
            (None, None) => Scope::All,
        }
    }

    /// A tenant by ID, name or domain (ignoring case), among the tenants
    /// known from Azure and from subscriptions. A GUID-looking ID is taken
    /// as is, so a tenant can be used before its name is known.
    fn find_tenant(&self, wanted: &str) -> Option<String> {
        let found = self.state().tenant_rows().into_iter().find_map(|row| {
            let id = row.id?;
            let hit = id.eq_ignore_ascii_case(wanted)
                || row.name.eq_ignore_ascii_case(wanted)
                || row
                    .domain
                    .as_deref()
                    .is_some_and(|d| d.eq_ignore_ascii_case(wanted));
            hit.then_some(id)
        });
        found.or_else(|| {
            let guid =
                wanted.len() == 36 && wanted.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
            guid.then(|| wanted.to_ascii_lowercase())
        })
    }

    /// IDs of all subscriptions that can be queried in the current tenant
    /// (disabled ones always fail).
    fn enabled_subscription_ids(&self) -> Vec<String> {
        let state = self.state();
        state
            .subscriptions
            .iter()
            .filter(|s| state.in_tenant(s))
            .filter(|s| !s.state.eq_ignore_ascii_case("Disabled"))
            .map(|s| s.id.clone())
            .collect()
    }

    /// IDs of the enabled subscriptions a cross-subscription view covers: all,
    /// or those below the management group the view was opened from.
    fn scope_subscription_ids(&self) -> Vec<String> {
        self.enabled_subscriptions_under(self.state().scope_group.as_deref())
    }

    /// Enabled subscriptions below `group` (all of them for `None`).
    fn enabled_subscriptions_under(&self, group: Option<&str>) -> Vec<String> {
        let state = self.state();
        let (Some(group), Some(tree)) = (group, &state.scope_tree) else {
            return self.enabled_subscription_ids();
        };
        let disabled: std::collections::HashSet<String> = state
            .subscriptions
            .iter()
            .filter(|s| s.state.eq_ignore_ascii_case("Disabled"))
            .map(|s| s.id.to_ascii_lowercase())
            .collect();
        tree.subscriptions_under(group, &state.subscriptions)
            .into_iter()
            .filter(|id| !disabled.contains(id))
            .collect()
    }

    /// Switch to the resource group list now and load it in the background.
    fn show_resource_groups(&mut self, subscription_ids: Vec<String>, all: bool) {
        let state = self.state_mut();
        state.view = ViewMode::ResourceGroupList;
        state.cross_subscription_mode = all;
        state.resource_query = Default::default();
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
        state.resource_query = Default::default();
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
        let state = self.state_mut();
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                debug!("Close help");
                state.toggle_help();
                state.help_scroll = 0;
            }
            // Scrolling; the drawing clamps the end.
            KeyCode::Down | KeyCode::Char('j') => state.help_scroll += 1,
            KeyCode::Up | KeyCode::Char('k') => {
                state.help_scroll = state.help_scroll.saturating_sub(1)
            }
            KeyCode::PageDown => state.help_scroll += 10,
            KeyCode::Char('d') if ctrl => state.help_scroll += 10,
            KeyCode::PageUp => state.help_scroll = state.help_scroll.saturating_sub(10),
            KeyCode::Char('u') if ctrl => state.help_scroll = state.help_scroll.saturating_sub(10),
            KeyCode::Char('g') => state.help_scroll = 0,
            KeyCode::Char('G') => state.help_scroll = usize::MAX / 2,
            _ => {}
        }
        Ok(true)
    }

    /// Auto-refresh (`refresh_interval` in the config): reload the current
    /// view when the interval has passed since the last load, unless a load
    /// is running or the user is typing, searching or reading help.
    pub async fn tick_at(&mut self, now: Instant) -> Result<()> {
        let state = self.state();
        let interval = state.config.refresh_interval;
        let busy = state.loading
            || state.command_mode
            || state.filter_mode
            || state.help_open
            || state.palette.is_some()
            || state.detail_search_input.is_some()
            || state.error.is_some();
        let refreshable = matches!(
            state.view,
            ViewMode::ManagementGroupTree
                | ViewMode::SubscriptionList
                | ViewMode::ResourceGroupList
                | ViewMode::ResourceList
                | ViewMode::ResourceTypeSummary
                | ViewMode::TagSummary
                | ViewMode::ResourceDetail
        );
        if interval > 0
            && !busy
            && refreshable
            && now.duration_since(self.last_refresh) >= Duration::from_secs(interval)
        {
            debug!("Auto-refresh after {interval}s");
            self.handle_refresh().await?;
        }
        Ok(())
    }

    /// Reload the current view in the background
    async fn handle_refresh(&mut self) -> Result<()> {
        self.last_refresh = Instant::now();
        let state = self.state();
        let scope_ids = if state.cross_subscription_mode {
            Some(self.scope_subscription_ids())
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
            (ViewMode::ResourceDetail, _) => self.core.refresh_resource_detail(),
            (ViewMode::TenantList, _) => self.core.request(LoadRequest::Tenants),
            (ViewMode::PolicyAssignments, _) => {
                if let Some(scope) = self.state().policy_scope.clone() {
                    self.core.open_policy(scope);
                }
            }
            (ViewMode::ManagementGroupTree, _) => {
                info!("Refreshing the management group tree");
                self.core.request(LoadRequest::ScopeTree);
            }
            (ViewMode::ResourceGroupList, Some(subscription_ids)) => {
                self.core
                    .request(LoadRequest::ResourceGroups { subscription_ids });
            }
            (
                ViewMode::ResourceList | ViewMode::ResourceTypeSummary | ViewMode::TagSummary,
                Some(subscription_ids),
            ) => {
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
            state.scope_group = ctx.metadata.get(SCOPE_GROUP).cloned();
            // Back to the policy view keeps its scope (still in state).
            state.resource_query = ctx
                .metadata
                .get(QUERY)
                .and_then(|q| ResourceQuery::parse(q.split_whitespace()).ok())
                .unwrap_or_default();
            if let Some(id) = ctx.metadata.get(SELECTED) {
                state.select_id(id);
            }
        }
        Ok(())
    }

    /// Navigate down in the current list
    fn navigate_down(&mut self) {
        if self.state().view == ViewMode::ResourceDetail {
            self.scroll_detail(1);
            return;
        }
        let state = self.state_mut();
        let list_len = state.current_list_len();
        if list_len > 0 {
            state.list_state.select_next(list_len);
        }
    }

    /// Navigate up in the current list
    fn navigate_up(&mut self) {
        if self.state().view == ViewMode::ResourceDetail {
            self.scroll_detail(-1);
            return;
        }
        let state = self.state_mut();
        if state.current_list_len() > 0 {
            state.list_state.select_previous();
        }
    }

    /// Navigate to first item
    fn navigate_first(&mut self) {
        if self.state().view == ViewMode::ResourceDetail {
            self.state_mut().detail_scroll = 0;
            return;
        }
        let state = self.state_mut();
        if state.current_list_len() > 0 {
            state.list_state.select_first();
        }
    }

    /// Navigate to last item
    fn navigate_last(&mut self) {
        if self.state().view == ViewMode::ResourceDetail {
            self.state_mut().detail_scroll = usize::MAX;
            return;
        }
        let state = self.state_mut();
        let list_len = state.current_list_len();
        if list_len > 0 {
            state.list_state.select_last(list_len);
        }
    }

    /// Navigate page down (viewport-aware)
    fn navigate_page_down(&mut self) {
        if self.state().view == ViewMode::ResourceDetail {
            self.scroll_detail(10);
            return;
        }
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
        if self.state().view == ViewMode::ResourceDetail {
            self.scroll_detail(-10);
            return;
        }
        let state = self.state_mut();
        if state.current_list_len() > 0 {
            // Jump by 10 items or to the start
            state.list_state.selected = state.list_state.selected.saturating_sub(10);
        }
    }

    /// Scroll the detail view by `lines` (the drawing clamps the end).
    fn scroll_detail(&mut self, lines: isize) {
        let state = self.state_mut();
        state.detail_scroll = state.detail_scroll.saturating_add_signed(lines);
    }

    /// Copy the selected item's ARM ID to the clipboard (`y`).
    fn copy_selected_id(&mut self) {
        let Some(id) = self.state().selected_arm_id() else {
            return;
        };
        let notice = match self.desktop.copy(&id) {
            Ok(()) => format!("Copied {id}"),
            Err(e) => format!("Could not copy ({e}): {id}"),
        };
        self.state_mut().notice = Some(notice);
    }

    /// The detail view's current lines as plain text, for searching.
    fn detail_texts(&self) -> Vec<String> {
        crate::ui::detail_lines(self.state())
            .iter()
            .map(|l| l.to_string())
            .collect()
    }

    /// Go to the match of `pattern` after (or before) `from`, scroll to it
    /// and say where it is. Returns whether one was found.
    fn search_jump(
        &mut self,
        pattern: &str,
        from: (usize, usize),
        forward: bool,
        inclusive: bool,
    ) -> bool {
        let matches = search::find_matches(&self.detail_texts(), pattern);
        let state = self.state_mut();
        match search::next_match(&matches, from, forward, inclusive) {
            Some((i, wrapped)) => {
                let m = matches[i];
                state.detail_match = Some((m.line, m.start));
                state.detail_scroll_to = Some(m.line);
                let position = format!("/{pattern}  match {} of {}", i + 1, matches.len());
                state.notice = Some(match (wrapped, forward) {
                    (true, true) => format!("search hit BOTTOM, continuing at TOP · {position}"),
                    (true, false) => format!("search hit TOP, continuing at BOTTOM · {position}"),
                    _ => position,
                });
                true
            }
            None => {
                state.detail_match = None;
                state.notice = Some(format!("Pattern not found: {pattern}"));
                false
            }
        }
    }

    /// `n` / `N`: next or previous match of the committed search.
    fn search_step(&mut self, forward: bool) {
        let pattern = self.state().detail_search.clone();
        if pattern.is_empty() {
            self.state_mut().notice = Some("No previous search (type / to search)".into());
            return;
        }
        let from = self
            .state()
            .detail_match
            .unwrap_or((self.state().detail_scroll, 0));
        // Without a current match, the first match at the top line counts.
        let inclusive = self.state().detail_match.is_none();
        self.search_jump(&pattern, from, forward, inclusive);
    }

    /// Keys while typing a search after `/`: incremental, like Vim's
    /// incsearch. Enter keeps the search, Esc cancels it.
    fn handle_search_key(&mut self, key: KeyEvent) {
        let state = self.state_mut();
        let Some(input) = state.detail_search_input.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Enter => {
                let pattern = state.detail_search_input.take().unwrap_or_default();
                if pattern.is_empty() {
                    // Like Vim: an empty pattern repeats the last search.
                    self.search_step(true);
                } else {
                    state.detail_search = pattern;
                }
                return;
            }
            KeyCode::Esc => {
                let (scroll, current) = state.detail_search_origin;
                state.detail_search_input = None;
                state.detail_scroll = scroll;
                state.detail_match = current;
                state.notice = None;
                return;
            }
            KeyCode::Backspace => {
                if input.pop().is_none() {
                    // Backspace on an empty search leaves search mode.
                    state.detail_search_input = None;
                    return;
                }
            }
            KeyCode::Char(c) => input.push(c),
            _ => return,
        }
        // Incremental: from where the search started, as the pattern grows.
        let pattern = input.clone();
        let (origin_scroll, _) = state.detail_search_origin;
        if pattern.is_empty() {
            state.detail_match = None;
            state.notice = None;
            return;
        }
        self.search_jump(&pattern, (origin_scroll, 0), true, true);
    }

    /// Copy what the current view shows (`c`): every row passing the
    /// filters, as tab-separated text, or the detail view's current tab.
    fn copy_view(&mut self) {
        let Some((what, text)) = az9s_core::export::view_text(self.state()) else {
            self.state_mut().notice = Some("Nothing to copy here".into());
            return;
        };
        let size = if text.len() >= 1024 {
            format!("{} KB", text.len() / 1024)
        } else {
            format!("{} bytes", text.len())
        };
        let notice = match self.desktop.copy(&text) {
            Ok(()) => format!("Copied {what} ({size})"),
            Err(e) => format!("Could not copy ({e})"),
        };
        self.state_mut().notice = Some(notice);
    }

    /// Open the selected item in the Azure Portal (`o`). The status bar
    /// shows the link either way, e.g. for SSH sessions without a browser.
    fn open_in_portal(&mut self) {
        let Some(id) = self.state().selected_arm_id() else {
            return;
        };
        let url = detail::portal_url(&id, self.state().portal_tenant().as_deref());
        let notice = match self.desktop.open(&url) {
            Ok(()) => "Opened in the Azure Portal".to_string(),
            // The link is too long for the status bar: put it on the
            // clipboard instead, so it can be pasted into a browser.
            Err(e) => match self.desktop.copy(&url) {
                Ok(()) => format!("No browser could be started; Portal link copied ({e})"),
                Err(_) => format!("No browser could be started ({e}): {url}"),
            },
        };
        self.state_mut().notice = Some(notice);
    }

    /// Handle selection (Enter key)
    async fn handle_select(&mut self) -> Result<bool> {
        match self.state().view {
            ViewMode::ManagementGroupTree => self.handle_tree_select(),
            ViewMode::SubscriptionList => self.handle_subscription_select().await,
            ViewMode::ResourceTypeSummary => {
                self.handle_type_select();
                Ok(true)
            }
            ViewMode::TagSummary => {
                self.handle_tag_select();
                Ok(true)
            }
            ViewMode::PolicyAssignments => {
                if let Some(row) = self.state().selected_policy_row() {
                    self.push_current_context(None);
                    let a = row.assignment;
                    let subscription_id = a
                        .scope
                        .to_ascii_lowercase()
                        .strip_prefix("/subscriptions/")
                        .map(|s| s.split('/').next().unwrap_or(s).to_string())
                        .unwrap_or_default();
                    self.core.open_resource_detail(az9s_core::models::Resource {
                        id: a.id,
                        name: a.display_name,
                        resource_type: "Microsoft.Authorization/policyAssignments".into(),
                        location: String::new(),
                        resource_group: String::new(),
                        subscription_id,
                        tags: Default::default(),
                    });
                }
                Ok(true)
            }
            ViewMode::TenantList => {
                if let Some(row) = self.state().selected_tenant_row() {
                    self.core.switch_tenant(row.id);
                }
                Ok(true)
            }
            ViewMode::ResourceGroupList => self.handle_resource_group_select().await,
            ViewMode::ResourceList => self.handle_resource_select().await,
            ViewMode::ResourceDetail => Ok(true),
            ViewMode::Logs => Ok(true),
        }
    }

    /// Enter on the tree: a subscription opens its resource groups; a group
    /// opens the resource groups of every enabled subscription below it.
    fn handle_tree_select(&mut self) -> Result<bool> {
        let Some(row) = self.state().selected_tree_row() else {
            return Ok(true);
        };
        if row.kind == TreeRowKind::Subscription {
            let sub = self
                .state()
                .subscriptions
                .iter()
                .find(|s| s.id.eq_ignore_ascii_case(&row.key))
                .cloned()
                .unwrap_or_else(|| az9s_core::models::Subscription {
                    id: row.key.clone(),
                    name: row.label.clone(),
                    state: "Enabled".into(),
                    tenant_id: String::new(),
                });
            self.push_current_context(None);
            let id = sub.id.clone();
            let state = self.state_mut();
            state.selected_subscription = Some(sub);
            state.scope_group = None;
            self.show_resource_groups(vec![id], false);
            return Ok(true);
        }

        let ids = self.enabled_subscriptions_under(Some(&row.key));
        if ids.is_empty() {
            self.state_mut().notice = Some(format!("No enabled subscriptions under {}", row.label));
            return Ok(true);
        }
        self.push_current_context(None);
        let state = self.state_mut();
        state.scope_group = Some(row.key.clone());
        state.selected_subscription = None;
        self.show_resource_groups(ids, true);
        Ok(true)
    }

    /// Open (`open = true`) or close the selected group in the tree. Closing
    /// a row that is already closed (or a subscription) moves to its parent.
    fn tree_expand(&mut self, open: Option<bool>) {
        let rows = self.state().tree_rows();
        let selected = self.state().list_state.selected;
        let Some(row) = rows.get(selected) else {
            return;
        };
        let open = open.unwrap_or(!row.expanded);
        if row.is_group() && row.has_children && row.expanded != open {
            self.state_mut().set_tree_expanded(&row.key, open);
        } else if !open && row.depth > 0 {
            if let Some(parent) = rows[..selected].iter().rposition(|r| r.depth < row.depth) {
                self.state_mut().list_state.selected = parent;
            }
        }
    }

    /// Enter on the type summary: the resource list, narrowed to that type
    /// (on top of any other filters). The rows are already loaded.
    fn handle_type_select(&mut self) {
        let Some(resource_type) = self.state().selected_type() else {
            return;
        };
        self.push_current_context(None);
        let state = self.state_mut();
        state.resource_query.types = vec![resource_type];
        state.filter.clear();
        state.view = ViewMode::ResourceList;
        state.list_state.reset();
    }

    /// Enter on the tags view: the resources with that tag (on top of any
    /// other filters). The rows are already loaded.
    fn handle_tag_select(&mut self) {
        let Some(tag) = self.state().selected_tag() else {
            return;
        };
        self.push_current_context(None);
        let state = self.state_mut();
        state.resource_query.tags = vec![(tag.key, Some(tag.value))];
        state.filter.clear();
        state.view = ViewMode::ResourceList;
        state.list_state.reset();
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
        if let Some(group) = &state.scope_group {
            metadata.insert(SCOPE_GROUP.to_string(), group.clone());
        }
        if !state.resource_query.is_empty() {
            metadata.insert(QUERY.to_string(), state.resource_query.to_string());
        }
        if let Some(id) = state.selected_id() {
            metadata.insert(SELECTED.to_string(), id);
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
            self.core.open_resource_detail(resource);
        }
        Ok(true)
    }

    /// Handle view-specific keys
    async fn handle_view_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('y') => {
                self.copy_selected_id();
                return Ok(true);
            }
            KeyCode::Char('o') => {
                self.open_in_portal();
                return Ok(true);
            }
            KeyCode::Char('c') => {
                self.copy_view();
                return Ok(true);
            }
            _ => {}
        }
        if self.state().view == ViewMode::ResourceDetail {
            let tab = self.state().detail_tab;
            let next = match key.code {
                KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => Some(tab.cycle(1)),
                KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => Some(tab.cycle(-1)),
                KeyCode::Char(c @ '1'..='3') => Some(DetailTab::ALL[c as usize - '1' as usize]),
                _ => None,
            };
            if let Some(next) = next.filter(|n| *n != tab) {
                let state = self.state_mut();
                state.detail_tab = next;
                state.detail_scroll = 0;
                state.detail_match = None;
            }
            match key.code {
                KeyCode::Char('n') => self.search_step(true),
                KeyCode::Char('N') => self.search_step(false),
                _ => {}
            }
            return Ok(true);
        }
        if self.state().view == ViewMode::ManagementGroupTree {
            match key.code {
                KeyCode::Char(' ') => self.tree_expand(None),
                KeyCode::Right | KeyCode::Char('l') => self.tree_expand(Some(true)),
                KeyCode::Left | KeyCode::Char('h') => self.tree_expand(Some(false)),
                _ => {}
            }
        }
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
