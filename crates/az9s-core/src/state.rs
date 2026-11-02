use crate::models::{Resource, ResourceGroup, Subscription};
use std::collections::HashMap;
use std::sync::Mutex;

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

    /// Error message (modal; dismissed with Esc)
    pub error: Option<String>,

    /// Non-blocking notice shown in the status bar (e.g. partial load failures)
    pub notice: Option<String>,

    /// Resource group whose resources are shown in the resource list, if any
    pub current_resource_group: Option<String>,

    /// Spinner animation frame, advanced on ticks while loading
    pub spinner_frame: usize,

    /// Where each list's data came from and when it was produced
    pub freshness: HashMap<ViewMode, Freshness>,

    /// Data source indicator (for UX transparency)
    pub data_source: DataSource,

    /// Cross-subscription mode (for "all" scope)
    pub cross_subscription_mode: bool,

    /// Memoized filter result, so a frame doesn't rescan every row
    filter_memo: Mutex<FilterMemo>,
}

/// Which list a memoized filter result belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Subscriptions,
    ResourceGroups,
    Resources,
}

/// Cached indices of the rows matching `filter`.
///
/// The key identifies the list by (kind, buffer address, length). Lists are only
/// ever replaced wholesale (`state.resources = new_vec`), and a replacement is
/// allocated while the old buffer is still alive, so it always gets a new
/// address. Mutating rows in place would not be detected; don't do that, or
/// call [`AppState::invalidate_filter`].
#[derive(Debug, Default)]
struct FilterMemo {
    key: Option<(ListKind, usize, usize)>,
    filter: String,
    indices: Vec<usize>,
}

/// Where a list's data came from and when (unix seconds): fetched from
/// Azure at that time, or read from the cache, written at that time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Freshness {
    pub source: DataSource,
    pub as_of: i64,
}

/// Indicates where the current data came from
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataSource {
    #[default]
    None,
    /// Data loaded fresh from Azure API
    Azure,
    /// Data loaded from local cache
    Cache,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
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

    /// Clamp the selection to `len`, scroll so it is visible in a viewport of
    /// `viewport` rows, and return the range of item indices to render.
    ///
    /// Only this range should be turned into widgets, so a frame costs
    /// O(viewport) instead of O(total rows).
    pub fn visible_range(&mut self, len: usize, viewport: usize) -> std::ops::Range<usize> {
        if len == 0 || viewport == 0 {
            self.selected = 0;
            self.offset = 0;
            return 0..0;
        }
        self.selected = self.selected.min(len - 1);
        self.update_offset(viewport);
        // Don't leave empty space at the bottom when the list shrank (e.g. filtering).
        self.offset = self.offset.min(len.saturating_sub(viewport));
        self.offset..(self.offset + viewport).min(len)
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
        self.filter_list(ListKind::Subscriptions, &self.subscriptions, |s, needle| {
            contains_ignore_ascii_case(&s.name, needle) || contains_ignore_ascii_case(&s.id, needle)
        })
    }

    /// Get filtered resource groups based on current filter
    pub fn filtered_resource_groups(&self) -> Vec<&ResourceGroup> {
        self.filter_list(
            ListKind::ResourceGroups,
            &self.resource_groups,
            |rg, needle| {
                contains_ignore_ascii_case(&rg.name, needle)
                    || contains_ignore_ascii_case(&rg.location, needle)
            },
        )
    }

    /// Get filtered resources based on current filter
    pub fn filtered_resources(&self) -> Vec<&Resource> {
        self.filter_list(ListKind::Resources, &self.resources, |r, needle| {
            contains_ignore_ascii_case(&r.name, needle)
                || contains_ignore_ascii_case(&r.resource_type, needle)
                || contains_ignore_ascii_case(&r.location, needle)
        })
    }

    /// Filter `items` by the current filter, reusing the previous result when
    /// neither the filter nor the list changed.
    fn filter_list<'a, T>(
        &self,
        kind: ListKind,
        items: &'a [T],
        matches: impl Fn(&T, &str) -> bool,
    ) -> Vec<&'a T> {
        if self.filter.is_empty() {
            return items.iter().collect();
        }
        let key = (kind, items.as_ptr() as usize, items.len());
        let mut memo = self.filter_memo.lock().unwrap_or_else(|e| e.into_inner());
        if memo.key != Some(key) || memo.filter != self.filter {
            let needle = self.filter.to_ascii_lowercase();
            memo.indices = items
                .iter()
                .enumerate()
                .filter(|(_, item)| matches(item, &needle))
                .map(|(i, _)| i)
                .collect();
            memo.key = Some(key);
            memo.filter.clone_from(&self.filter);
        }
        memo.indices.iter().map(|&i| &items[i]).collect()
    }

    /// Drop the memoized filter result (only needed after in-place row edits).
    pub fn invalidate_filter(&self) {
        let mut memo = self.filter_memo.lock().unwrap_or_else(|e| e.into_inner());
        memo.key = None;
    }

    /// Freshness of the list behind the current view (a resource's detail
    /// view uses the resource list's).
    pub fn current_freshness(&self) -> Option<Freshness> {
        let view = match self.view {
            ViewMode::ResourceDetail => ViewMode::ResourceList,
            ref v => v.clone(),
        };
        self.freshness.get(&view).copied()
    }

    /// ID of the selected row in the current list view.
    pub fn selected_id(&self) -> Option<String> {
        match self.view {
            ViewMode::SubscriptionList => self.selected_subscription().map(|s| s.id.clone()),
            ViewMode::ResourceGroupList => self.selected_resource_group().map(|r| r.id.clone()),
            ViewMode::ResourceList => self.selected_resource().map(|r| r.id.clone()),
            _ => None,
        }
    }

    /// Select the row with this ID in the current (filtered) list, if present.
    pub fn select_id(&mut self, id: &str) {
        let position = match self.view {
            ViewMode::SubscriptionList => self
                .filtered_subscriptions()
                .iter()
                .position(|s| s.id == id),
            ViewMode::ResourceGroupList => self
                .filtered_resource_groups()
                .iter()
                .position(|r| r.id == id),
            ViewMode::ResourceList => self.filtered_resources().iter().position(|r| r.id == id),
            _ => None,
        };
        if let Some(i) = position {
            self.list_state.selected = i;
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

/// How long ago a unix timestamp was, for the status bar: "just now",
/// "42s ago", "5m ago", "3h ago", "2d ago".
pub fn format_age(as_of: i64) -> String {
    format_age_at(as_of, chrono::Utc::now().timestamp())
}

pub fn format_age_at(as_of: i64, now: i64) -> String {
    let secs = (now - as_of).max(0);
    match secs {
        0..=9 => "just now".to_string(),
        10..=59 => format!("{secs}s ago"),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86_399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

/// Case-insensitive (ASCII) substring check that does not allocate.
///
/// `needle_lower` must already be ASCII-lowercased. Azure names, types and
/// locations are ASCII in practice; non-ASCII bytes are compared exactly.
pub fn contains_ignore_ascii_case(haystack: &str, needle_lower: &str) -> bool {
    let (h, n) = (haystack.as_bytes(), needle_lower.as_bytes());
    if n.is_empty() {
        return true;
    }
    if n.len() > h.len() {
        return false;
    }
    h.windows(n.len())
        .any(|w| w.iter().zip(n).all(|(a, b)| a.to_ascii_lowercase() == *b))
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
    fn test_format_age() {
        assert_eq!(format_age_at(100, 105), "just now");
        assert_eq!(format_age_at(100, 142), "42s ago");
        assert_eq!(format_age_at(0, 5 * 60 + 30), "5m ago");
        assert_eq!(format_age_at(0, 3 * 3600 + 5), "3h ago");
        assert_eq!(format_age_at(0, 2 * 86_400), "2d ago");
        assert_eq!(
            format_age_at(200, 100),
            "just now",
            "clock skew never goes negative"
        );
    }

    #[test]
    fn test_contains_ignore_ascii_case() {
        assert!(contains_ignore_ascii_case("MyResourceGroup", "resource"));
        assert!(contains_ignore_ascii_case(
            "westeurope",
            "WESTEUROPE".to_ascii_lowercase().as_str()
        ));
        assert!(contains_ignore_ascii_case("anything", ""));
        assert!(!contains_ignore_ascii_case("abc", "abcd"));
        assert!(!contains_ignore_ascii_case("rg-prod", "dev"));
        assert!(contains_ignore_ascii_case("zażółć-rg", "ółć-rg"));
    }

    #[test]
    fn test_visible_range_scrolls_and_persists_offset() {
        let mut ls = ListState::new();
        // Move to the bottom of a 100-row list in a 10-row viewport.
        ls.selected = 99;
        assert_eq!(ls.visible_range(100, 10), 90..100);
        // Moving up inside the viewport must not scroll (offset persists).
        ls.selected = 95;
        assert_eq!(ls.visible_range(100, 10), 90..100);
        // Moving above the viewport scrolls up just enough.
        ls.selected = 85;
        assert_eq!(ls.visible_range(100, 10), 85..95);
    }

    #[test]
    fn test_visible_range_clamps_when_list_shrinks() {
        let mut ls = ListState::new();
        ls.selected = 50;
        ls.visible_range(100, 10);
        // Filter reduced the list to 5 items.
        assert_eq!(ls.visible_range(5, 10), 0..5);
        assert_eq!(ls.selected, 4);
        assert_eq!(ls.visible_range(0, 10), 0..0);
        assert_eq!(ls.selected, 0);
    }

    fn resource(name: &str) -> Resource {
        Resource {
            id: format!("/subscriptions/s/resourceGroups/rg/providers/x/{name}"),
            name: name.into(),
            resource_type: "Microsoft.Compute/virtualMachines".into(),
            location: "westeurope".into(),
            resource_group: "rg".into(),
            subscription_id: "s".into(),
            tags: HashMap::new(),
        }
    }

    #[test]
    fn test_filter_memo_tracks_filter_and_list_changes() {
        let mut state = AppState::new();
        state.resources = vec![resource("web-1"), resource("db-1"), resource("web-2")];

        state.filter = "web".into();
        assert_eq!(state.filtered_resources().len(), 2);
        // Same filter and list: served from the memo, same answer.
        assert_eq!(state.filtered_resources().len(), 2);

        // Filter changes.
        state.filter = "db".into();
        assert_eq!(state.filtered_resources()[0].name, "db-1");

        // List replaced, even with the same length.
        state.resources = vec![resource("db-9"), resource("x"), resource("db-8")];
        let names: Vec<_> = state
            .filtered_resources()
            .iter()
            .map(|r| r.name.clone())
            .collect();
        assert_eq!(names, ["db-9", "db-8"]);

        // Different list kinds don't share results.
        state.subscriptions = vec![];
        assert!(state.filtered_subscriptions().is_empty());
        assert_eq!(state.filtered_resources().len(), 2);
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
