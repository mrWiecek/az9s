use crate::models::{Resource, ResourceGroup, Subscription};
use crate::query::ResourceQuery;
use crate::scope_tree::{ScopeTree, TreeRow};
use std::collections::{HashMap, HashSet};
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

    /// Full document of the resource in the detail view, once loaded
    pub resource_document: Option<serde_json::Value>,

    /// Active tab of the detail view
    pub detail_tab: DetailTab,

    /// First visible line of the detail view's content
    pub detail_scroll: usize,

    /// Committed search in the detail view (`/pattern` + Enter); `n`/`N` use it
    pub detail_search: String,

    /// The search being typed after `/` in the detail view, if any
    pub detail_search_input: Option<String>,

    /// Where the search started (scroll, current match), restored by Esc
    pub detail_search_origin: (usize, Option<(usize, usize)>),

    /// The current search match: (line, byte offset)
    pub detail_match: Option<(usize, usize)>,

    /// A line the detail view must scroll to show (set by search, applied
    /// when drawing, since only drawing knows the viewport height)
    pub detail_scroll_to: Option<usize>,

    /// Navigation history
    pub navigation_stack: Vec<NavigationContext>,

    /// Command mode state (k9s-like colon commands)
    pub command_mode: bool,

    /// Command input buffer
    pub command_input: String,

    /// Selected autocomplete suggestion in command mode
    pub command_selection: usize,

    /// The command palette (Ctrl+P), while open
    pub palette: Option<crate::palette::Palette>,

    /// Settings from the config file
    pub config: crate::config::Config,

    /// Tenants the user can access (names from Azure), once loaded
    pub tenants: Vec<crate::models::Tenant>,

    /// Every policy assignment in the tenant, once loaded
    pub policy_assignments: Vec<crate::policy::PolicyAssignment>,

    /// Compliance counts per assignment (lowercase ID) for a scope
    pub policy_compliance: Option<(
        crate::policy::PolicyScope,
        HashMap<String, crate::policy::Compliance>,
    )>,

    /// The scope the policy view shows
    pub policy_scope: Option<crate::policy::PolicyScope>,

    /// The assignment whose non-compliant resources are shown
    pub policy_states_assignment: Option<crate::policy::PolicyAssignment>,

    /// Non-compliant policy states of that assignment, once loaded
    pub policy_states: Vec<crate::policy::PolicyState>,

    /// Policy states of the resource in the detail view: (resource ID, states)
    pub resource_policy: Option<(String, Vec<crate::policy::PolicyState>)>,

    /// The tenant az9s works in (`:ctx`); `None`: the Azure CLI's default
    /// login, with subscriptions of every tenant listed
    pub tenant: Option<String>,

    /// Help screen state
    pub help_open: bool,

    /// First visible line of the help screen
    pub help_scroll: usize,

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

    /// Management group hierarchy, once loaded
    pub scope_tree: Option<ScopeTree>,

    /// Groups that are open in the tree view
    pub tree_expanded: HashSet<String>,

    /// Management group whose subscriptions the cross-subscription lists show
    /// (`None`: all subscriptions)
    pub scope_group: Option<String>,

    /// Structured filter on the current resource or resource group list
    /// (`:res type=… loc=… tag=…`, `:rg loc=… tag=…`)
    pub resource_query: ResourceQuery,

    /// Memoized filter result, so a frame doesn't rescan every row
    filter_memo: Mutex<FilterMemo>,

    /// Memoized resource-type summary: (list key, query) → counts
    type_summary_memo: Mutex<Option<TypeSummaryMemo>>,

    /// Memoized tag counts, like `type_summary_memo`
    tag_summary_memo: Mutex<Option<TagSummaryMemo>>,
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

/// Memoized type counts: which resource list (buffer address, length) and
/// query they were counted for, and the counts.
#[derive(Debug)]
struct TypeSummaryMemo {
    key: (usize, usize, String),
    rows: Vec<(String, usize)>,
}

/// One row of the tags view: a tag key and value, and how many resources
/// carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    pub key: String,
    pub value: String,
    pub count: usize,
}

/// One row of the policy view (`:policy`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRow {
    pub assignment: crate::policy::PolicyAssignment,
    /// Assigned at an ancestor rather than at this scope
    pub inherited: bool,
    /// "this scope" or "from <scope name>"
    pub origin_label: String,
    /// Resources evaluated / non-compliant, once loaded
    pub compliance: Option<crate::policy::Compliance>,
}

/// One row of the tenant view (`:ctx`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantRow {
    /// `None`: all tenants (the CLI's default login)
    pub id: Option<String>,
    pub name: String,
    pub domain: Option<String>,
    pub subscriptions: usize,
    /// The tenant az9s works in now
    pub current: bool,
}

/// Memoized tag counts for one resource list and query.
#[derive(Debug)]
struct TagSummaryMemo {
    key: (usize, usize, String),
    rows: Vec<TagCount>,
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
    ManagementGroupTree,
    ResourceGroupList,
    ResourceList,
    /// Resource counts per type (`:type`)
    ResourceTypeSummary,
    /// Tag key/value counts (`:tag`)
    TagSummary,
    /// Tenants to switch to (`:ctx`)
    TenantList,
    /// Policy assignments that apply at a scope (`:policy`)
    PolicyAssignments,
    /// Non-compliant resources of one policy assignment
    PolicyNonCompliant,
    ResourceDetail,
    Logs,
}

/// Tabs of the resource detail view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailTab {
    #[default]
    Overview,
    Tags,
    Policy,
    Json,
}

impl DetailTab {
    pub const ALL: [DetailTab; 4] = [
        DetailTab::Overview,
        DetailTab::Tags,
        DetailTab::Policy,
        DetailTab::Json,
    ];

    pub fn title(self) -> &'static str {
        match self {
            DetailTab::Overview => "Overview",
            DetailTab::Tags => "Tags",
            DetailTab::Policy => "Policy",
            DetailTab::Json => "JSON",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    /// The tab `step` places to the right (negative: left), wrapping.
    pub fn cycle(self, step: isize) -> Self {
        let n = Self::ALL.len() as isize;
        Self::ALL[(self.index() as isize + step).rem_euclid(n) as usize]
    }
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
            ViewMode::ResourceTypeSummary => self.type_summary().len(),
            ViewMode::TagSummary => self.tag_summary().len(),
            ViewMode::TenantList => self.tenant_rows().len(),
            ViewMode::PolicyAssignments => self.policy_rows().len(),
            ViewMode::PolicyNonCompliant => self.non_compliant_rows().len(),
            ViewMode::ManagementGroupTree => self.tree_rows().len(),
            _ => 0,
        }
    }

    /// Visible rows of the management group tree (expansion and filter applied).
    pub fn tree_rows(&self) -> Vec<TreeRow> {
        match &self.scope_tree {
            Some(tree) => tree.rows(&self.tree_expanded, &self.subscriptions, &self.filter),
            None => Vec::new(),
        }
    }

    /// The selected row of the tree view.
    pub fn selected_tree_row(&self) -> Option<TreeRow> {
        if self.view != ViewMode::ManagementGroupTree {
            return None;
        }
        self.tree_rows().into_iter().nth(self.list_state.selected)
    }

    /// Store a freshly loaded tree. The first tree opens its root group.
    pub fn set_scope_tree(&mut self, tree: ScopeTree) {
        if self.scope_tree.is_none() {
            self.tree_expanded.insert(tree.root.clone());
        }
        self.scope_tree = Some(tree);
    }

    /// Open or close a group in the tree view.
    pub fn set_tree_expanded(&mut self, key: &str, open: bool) {
        if open {
            self.tree_expanded.insert(key.to_string());
        } else {
            self.tree_expanded.remove(key);
        }
    }

    /// Get filtered subscriptions based on current filter
    pub fn filtered_subscriptions(&self) -> Vec<&Subscription> {
        // The current tenant is part of the memo key.
        let signature = self.tenant.clone().unwrap_or_default();
        self.filter_list_with(
            ListKind::Subscriptions,
            &self.subscriptions,
            &signature,
            |s, needle| {
                self.in_tenant(s)
                    && (needle.is_empty()
                        || contains_ignore_ascii_case(&s.name, needle)
                        || contains_ignore_ascii_case(&s.id, needle))
            },
        )
    }

    /// Whether a subscription belongs to the current tenant (always, with
    /// no tenant chosen).
    pub fn in_tenant(&self, sub: &Subscription) -> bool {
        self.tenant
            .as_ref()
            .is_none_or(|t| sub.tenant_id.eq_ignore_ascii_case(t))
    }

    /// The current tenant's name (or ID, before names load).
    pub fn tenant_label(&self) -> Option<String> {
        let id = self.tenant.as_ref()?;
        Some(
            self.tenants
                .iter()
                .find(|t| t.id.eq_ignore_ascii_case(id))
                .map(|t| t.name.clone())
                .unwrap_or_else(|| id.clone()),
        )
    }

    /// Rows of the tenant view: "all tenants" first, then every tenant known
    /// from Azure or from a subscription, with its subscription count.
    pub fn tenant_rows(&self) -> Vec<TenantRow> {
        let mut rows: Vec<TenantRow> = self
            .tenants
            .iter()
            .map(|t| TenantRow {
                id: Some(t.id.clone()),
                name: t.name.clone(),
                domain: t.domain.clone(),
                subscriptions: 0,
                current: false,
            })
            .collect();
        for sub in &self.subscriptions {
            match rows.iter_mut().find(|r| {
                r.id.as_deref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(&sub.tenant_id))
            }) {
                Some(row) => row.subscriptions += 1,
                None if !sub.tenant_id.is_empty() => rows.push(TenantRow {
                    id: Some(sub.tenant_id.clone()),
                    name: sub.tenant_id.clone(),
                    domain: None,
                    subscriptions: 1,
                    current: false,
                }),
                None => {}
            }
        }
        rows.sort_by_cached_key(|r| r.name.to_lowercase());
        rows.insert(
            0,
            TenantRow {
                id: None,
                name: "All tenants".into(),
                domain: Some("the Azure CLI's default login".into()),
                subscriptions: self.subscriptions.len(),
                current: false,
            },
        );
        for row in &mut rows {
            row.current = match (&row.id, &self.tenant) {
                (None, None) => true,
                (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                _ => false,
            };
        }
        if self.filter.is_empty() {
            return rows;
        }
        let needle = self.filter.to_ascii_lowercase();
        rows.into_iter()
            .filter(|r| {
                contains_ignore_ascii_case(&r.name, &needle)
                    || r.id
                        .as_deref()
                        .is_some_and(|id| contains_ignore_ascii_case(id, &needle))
                    || r.domain
                        .as_deref()
                        .is_some_and(|d| contains_ignore_ascii_case(d, &needle))
            })
            .collect()
    }

    /// Rows of the policy view: the assignments that apply at the policy
    /// scope (assigned there or inherited), with compliance counts when
    /// loaded, most non-compliant first. `/` narrows by name or origin.
    pub fn policy_rows(&self) -> Vec<PolicyRow> {
        use crate::policy::{applicable, Origin};
        let Some(scope) = &self.policy_scope else {
            return vec![];
        };
        let counts = self
            .policy_compliance
            .as_ref()
            .filter(|(s, _)| s == scope)
            .map(|(_, c)| c);
        let mut rows: Vec<PolicyRow> =
            applicable(&self.policy_assignments, scope, self.scope_tree.as_ref())
                .into_iter()
                .map(|(a, origin)| PolicyRow {
                    origin_label: match &origin {
                        Origin::Here => "this scope".into(),
                        Origin::Inherited(scope) => format!("from {}", self.scope_label(scope)),
                    },
                    compliance: counts.and_then(|c| c.get(&a.id.to_ascii_lowercase()).copied()),
                    assignment: a.clone(),
                    inherited: origin != Origin::Here,
                })
                .collect();
        rows.sort_by(|a, b| {
            let nc = |r: &PolicyRow| r.compliance.map_or(0, |c| c.non_compliant);
            nc(b).cmp(&nc(a)).then_with(|| {
                a.assignment
                    .display_name
                    .to_lowercase()
                    .cmp(&b.assignment.display_name.to_lowercase())
            })
        });
        if self.filter.is_empty() {
            return rows;
        }
        let needle = self.filter.to_ascii_lowercase();
        rows.into_iter()
            .filter(|r| {
                contains_ignore_ascii_case(&r.assignment.display_name, &needle)
                    || contains_ignore_ascii_case(&r.origin_label, &needle)
            })
            .collect()
    }

    /// The selected row of the policy view.
    pub fn selected_policy_row(&self) -> Option<PolicyRow> {
        if self.view != ViewMode::PolicyAssignments {
            return None;
        }
        self.policy_rows().into_iter().nth(self.list_state.selected)
    }

    /// Rows of the non-compliant view: one per resource, `/` narrows by
    /// name, type, resource group or failing policy.
    pub fn non_compliant_rows(&self) -> Vec<crate::policy::NonCompliantResource> {
        let rows = crate::policy::group_by_resource(&self.policy_states);
        if self.filter.is_empty() {
            return rows;
        }
        let needle = self.filter.to_ascii_lowercase();
        rows.into_iter()
            .filter(|r| {
                contains_ignore_ascii_case(r.state.resource_name(), &needle)
                    || contains_ignore_ascii_case(&r.state.resource_type, &needle)
                    || contains_ignore_ascii_case(&r.state.resource_group, &needle)
                    || r.policies
                        .iter()
                        .any(|p| contains_ignore_ascii_case(p, &needle))
            })
            .collect()
    }

    /// The selected row of the non-compliant view.
    pub fn selected_non_compliant(&self) -> Option<crate::policy::NonCompliantResource> {
        if self.view != ViewMode::PolicyNonCompliant {
            return None;
        }
        self.non_compliant_rows()
            .into_iter()
            .nth(self.list_state.selected)
    }

    /// Policy states of the resource in the detail view, once loaded:
    /// non-compliant first, then by assignment name and policy.
    pub fn detail_policy_states(&self) -> Option<Vec<(String, crate::policy::PolicyState)>> {
        let resource = self.selected_resource_detail.as_ref()?;
        let (id, states) = self.resource_policy.as_ref()?;
        if !id.eq_ignore_ascii_case(&resource.id) {
            return None;
        }
        let mut rows: Vec<(String, crate::policy::PolicyState)> = states
            .iter()
            .map(|s| (self.assignment_name(&s.assignment_id), s.clone()))
            .collect();
        rows.sort_by(|(an, a), (bn, b)| {
            b.is_non_compliant()
                .cmp(&a.is_non_compliant())
                .then_with(|| an.to_lowercase().cmp(&bn.to_lowercase()))
                .then_with(|| a.definition.cmp(&b.definition))
        });
        Some(rows)
    }

    /// An assignment's display name when loaded, else the last part of its ID.
    pub fn assignment_name(&self, id: &str) -> String {
        self.policy_assignments
            .iter()
            .find(|a| a.id.eq_ignore_ascii_case(id))
            .map(|a| a.display_name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| id.rsplit('/').next().unwrap_or(id).to_string())
    }

    /// A readable name for a scope's ARM ID: a management group's or
    /// subscription's display name when known.
    pub fn scope_label(&self, arm_id: &str) -> String {
        let lower = arm_id.to_ascii_lowercase();
        if let Some(group) = lower.strip_prefix("/providers/microsoft.management/managementgroups/")
        {
            if let Some(name) = self.scope_tree.as_ref().and_then(|t| {
                t.label_of(group).or_else(|| {
                    t.groups
                        .iter()
                        .find(|g| g.name.eq_ignore_ascii_case(group))
                        .map(|g| g.display_name.as_str())
                })
            }) {
                return name.to_string();
            }
            return group.to_string();
        }
        if let Some(sub) = lower.strip_prefix("/subscriptions/") {
            let id = sub.split('/').next().unwrap_or(sub);
            if let Some(s) = self
                .subscriptions
                .iter()
                .find(|s| s.id.eq_ignore_ascii_case(id))
            {
                return s.name.clone();
            }
        }
        arm_id.to_string()
    }

    /// The selected row of the tenant view.
    pub fn selected_tenant_row(&self) -> Option<TenantRow> {
        if self.view != ViewMode::TenantList {
            return None;
        }
        self.tenant_rows().into_iter().nth(self.list_state.selected)
    }

    /// Get filtered resource groups based on current filter
    pub fn filtered_resource_groups(&self) -> Vec<&ResourceGroup> {
        let query = &self.resource_query;
        let signature = query.to_string();
        self.filter_list_with(
            ListKind::ResourceGroups,
            &self.resource_groups,
            &signature,
            |rg, needle| {
                query.matches_group(rg)
                    && (needle.is_empty()
                        || contains_ignore_ascii_case(&rg.name, needle)
                        || contains_ignore_ascii_case(&rg.location, needle))
            },
        )
    }

    /// Get filtered resources based on current filter
    /// Resources matching the structured query and the `/` text filter.
    pub fn filtered_resources(&self) -> Vec<&Resource> {
        let query = &self.resource_query;
        let signature = query.to_string();
        self.filter_list_with(
            ListKind::Resources,
            &self.resources,
            &signature,
            |r, needle| {
                query.matches(r)
                    && (needle.is_empty()
                        || contains_ignore_ascii_case(&r.name, needle)
                        || crate::resource_type::type_contains(&r.resource_type, needle)
                        || contains_ignore_ascii_case(&r.location, needle))
            },
        )
    }

    /// Resource counts per type (types lowercase, most common first) over the
    /// resources matching the structured query, narrowed by the `/` filter
    /// on the type name.
    pub fn type_summary(&self) -> Vec<(String, usize)> {
        let key = (
            self.resources.as_ptr() as usize,
            self.resources.len(),
            self.resource_query.to_string(),
        );
        let mut memo = self
            .type_summary_memo
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if memo.as_ref().map(|m| &m.key) != Some(&key) {
            let mut counts: HashMap<String, usize> = HashMap::new();
            for r in self
                .resources
                .iter()
                .filter(|r| self.resource_query.matches(r))
            {
                *counts
                    .entry(r.resource_type.to_ascii_lowercase())
                    .or_default() += 1;
            }
            let mut rows: Vec<(String, usize)> = counts.into_iter().collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            *memo = Some(TypeSummaryMemo { key, rows });
        }
        let rows = &memo.as_ref().expect("set above").rows;
        if self.filter.is_empty() {
            return rows.clone();
        }
        let needle = self.filter.to_ascii_lowercase();
        rows.iter()
            .filter(|(t, _)| crate::resource_type::type_contains(t, &needle))
            .cloned()
            .collect()
    }

    /// Tags on the resources matching the structured query: one row per
    /// key and value, most common first. Keys are grouped ignoring case (as
    /// Azure treats them) and shown as first seen; values are kept as they
    /// are. The `/` filter narrows rows by key or value.
    pub fn tag_summary(&self) -> Vec<TagCount> {
        let key = (
            self.resources.as_ptr() as usize,
            self.resources.len(),
            self.resource_query.to_string(),
        );
        let mut memo = self
            .tag_summary_memo
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if memo.as_ref().map(|m| &m.key) != Some(&key) {
            // (lowercase key, value) -> (key as first seen, count)
            let mut counts: HashMap<(String, String), (String, usize)> = HashMap::new();
            for r in self
                .resources
                .iter()
                .filter(|r| self.resource_query.matches(r))
            {
                for (k, v) in &r.tags {
                    counts
                        .entry((k.to_lowercase(), v.clone()))
                        .or_insert_with(|| (k.clone(), 0))
                        .1 += 1;
                }
            }
            let mut rows: Vec<TagCount> = counts
                .into_iter()
                .map(|((_, value), (key, count))| TagCount { key, value, count })
                .collect();
            rows.sort_by(|a, b| {
                b.count
                    .cmp(&a.count)
                    .then_with(|| a.key.to_lowercase().cmp(&b.key.to_lowercase()))
                    .then_with(|| a.value.cmp(&b.value))
            });
            *memo = Some(TagSummaryMemo { key, rows });
        }
        let rows = &memo.as_ref().expect("set above").rows;
        if self.filter.is_empty() {
            return rows.clone();
        }
        let needle = self.filter.to_ascii_lowercase();
        rows.iter()
            .filter(|t| {
                contains_ignore_ascii_case(&t.key, &needle)
                    || contains_ignore_ascii_case(&t.value, &needle)
            })
            .cloned()
            .collect()
    }

    /// The selected row of the tags view.
    pub fn selected_tag(&self) -> Option<TagCount> {
        if self.view != ViewMode::TagSummary {
            return None;
        }
        self.tag_summary().into_iter().nth(self.list_state.selected)
    }

    /// The selected row of the type summary.
    pub fn selected_type(&self) -> Option<String> {
        if self.view != ViewMode::ResourceTypeSummary {
            return None;
        }
        self.type_summary()
            .into_iter()
            .nth(self.list_state.selected)
            .map(|(t, _)| t)
    }

    /// Filter `items` by the current `/` text plus an extra filter identified
    /// by `signature` (part of the memo key; empty means none), reusing the
    /// previous result when neither changed. `matches` gets the lowercase
    /// text filter, which may be empty.
    fn filter_list_with<'a, T>(
        &self,
        kind: ListKind,
        items: &'a [T],
        signature: &str,
        matches: impl Fn(&T, &str) -> bool,
    ) -> Vec<&'a T> {
        if self.filter.is_empty() && signature.is_empty() {
            return items.iter().collect();
        }
        let key = (kind, items.as_ptr() as usize, items.len());
        let memo_filter = format!("{signature}\u{0}{}", self.filter);
        let mut memo = self.filter_memo.lock().unwrap_or_else(|e| e.into_inner());
        if memo.key != Some(key) || memo.filter != memo_filter {
            let needle = self.filter.to_ascii_lowercase();
            memo.indices = items
                .iter()
                .enumerate()
                .filter(|(_, item)| matches(item, &needle))
                .map(|(i, _)| i)
                .collect();
            memo.key = Some(key);
            memo.filter = memo_filter;
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
        match self.view {
            // Until its document loads, the detail view shows the list's row.
            ViewMode::ResourceDetail if self.resource_document.is_none() => {
                self.freshness.get(&ViewMode::ResourceList).copied()
            }
            // The summary counts the resource list.
            ViewMode::ResourceTypeSummary | ViewMode::TagSummary => {
                self.freshness.get(&ViewMode::ResourceList).copied()
            }
            ref v => self.freshness.get(v).copied(),
        }
    }

    /// Full ARM ID of what is selected (or open, in the detail view): for
    /// `y` (copy) and `o` (open in the Portal).
    pub fn selected_arm_id(&self) -> Option<String> {
        match self.view {
            ViewMode::SubscriptionList => self
                .selected_subscription()
                .map(|s| format!("/subscriptions/{}", s.id)),
            ViewMode::ResourceGroupList => self.selected_resource_group().map(|r| r.id.clone()),
            ViewMode::ResourceList => self.selected_resource().map(|r| r.id.clone()),
            ViewMode::ResourceDetail => {
                self.selected_resource_detail.as_ref().map(|r| r.id.clone())
            }
            ViewMode::ManagementGroupTree => {
                let row = self.selected_tree_row()?;
                match row.kind {
                    crate::scope_tree::TreeRowKind::ManagementGroup => Some(format!(
                        "/providers/Microsoft.Management/managementGroups/{}",
                        row.key
                    )),
                    crate::scope_tree::TreeRowKind::Subscription => {
                        Some(format!("/subscriptions/{}", row.key))
                    }
                    crate::scope_tree::TreeRowKind::Unplaced => None,
                }
            }
            ViewMode::PolicyAssignments => self.selected_policy_row().map(|r| r.assignment.id),
            ViewMode::PolicyNonCompliant => {
                self.selected_non_compliant().map(|r| r.state.resource_id)
            }
            ViewMode::ResourceTypeSummary
            | ViewMode::TagSummary
            | ViewMode::TenantList
            | ViewMode::Logs => None,
        }
    }

    /// Tenant to open Portal links in: the selected subscription's, or the
    /// tree's root (the tenant root group's name is the tenant ID).
    pub fn portal_tenant(&self) -> Option<String> {
        self.selected_subscription
            .as_ref()
            .map(|s| s.tenant_id.clone())
            .filter(|t| !t.is_empty())
            .or_else(|| self.scope_tree.as_ref().map(|t| t.root.clone()))
    }

    /// ID of the selected row in the current list view.
    pub fn selected_id(&self) -> Option<String> {
        match self.view {
            ViewMode::SubscriptionList => self.selected_subscription().map(|s| s.id.clone()),
            ViewMode::ResourceGroupList => self.selected_resource_group().map(|r| r.id.clone()),
            ViewMode::ResourceList => self.selected_resource().map(|r| r.id.clone()),
            ViewMode::ManagementGroupTree => self.selected_tree_row().map(|r| r.key),
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
            ViewMode::ManagementGroupTree => self.tree_rows().iter().position(|r| r.key == id),
            ViewMode::PolicyAssignments => self
                .policy_rows()
                .iter()
                .position(|r| r.assignment.id == id),
            ViewMode::PolicyNonCompliant => self
                .non_compliant_rows()
                .iter()
                .position(|r| r.state.resource_id == id),
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

    /// The pattern to highlight in the detail view: the one being typed, or
    /// the committed one.
    pub fn detail_pattern(&self) -> &str {
        self.detail_search_input
            .as_deref()
            .unwrap_or(&self.detail_search)
    }

    /// Forget the detail view's search (new resource, or Esc).
    pub fn clear_detail_search(&mut self) {
        self.detail_search.clear();
        self.detail_search_input = None;
        self.detail_match = None;
        self.detail_scroll_to = None;
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
        self.command_selection = 0;
    }

    /// Exit command mode
    pub fn exit_command_mode(&mut self) {
        self.command_mode = false;
        self.command_input.clear();
    }

    /// Add character to command input
    pub fn command_push(&mut self, c: char) {
        self.command_input.push(c);
        self.command_selection = 0;
    }

    /// Remove last character from command input
    pub fn command_pop(&mut self) {
        self.command_input.pop();
        self.command_selection = 0;
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
    fn tag_summary_counts_key_value_pairs_grouping_keys_ignoring_case() {
        let mut state = AppState::new();
        let tagged = |name: &str, tags: &[(&str, &str)]| Resource {
            tags: tags
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..resource(name)
        };
        state.resources = vec![
            tagged("a", &[("env", "prod"), ("owner", "team-a")]),
            tagged("b", &[("Env", "prod")]),
            tagged("c", &[("env", "dev")]),
            tagged("d", &[]),
        ];
        state.view = ViewMode::TagSummary;
        let rows: Vec<(String, String, usize)> = state
            .tag_summary()
            .into_iter()
            .map(|t| (t.key.to_lowercase(), t.value, t.count))
            .collect();
        assert_eq!(
            rows,
            [
                ("env".into(), "prod".into(), 2),
                ("env".into(), "dev".into(), 1),
                ("owner".into(), "team-a".into(), 1),
            ]
        );
        assert_eq!(state.selected_tag().unwrap().value, "prod");

        state.filter = "team".into();
        assert_eq!(state.tag_summary().len(), 1, "/ matches values too");
        state.filter.clear();
        state.resource_query = ResourceQuery::parse(["tag=owner"]).unwrap();
        assert_eq!(state.tag_summary().len(), 2, "follows the query: only a");
    }

    #[test]
    fn structured_query_combines_with_text_filter_and_feeds_the_type_summary() {
        let mut state = AppState::new();
        let mk = |name: &str, t: &str| Resource {
            resource_type: t.into(),
            ..resource(name)
        };
        state.resources = vec![
            mk("vm-a", "Microsoft.Compute/virtualMachines"),
            mk("vm-b", "microsoft.compute/virtualmachines"),
            mk("disk-a", "Microsoft.Compute/disks"),
            mk("kv", "Microsoft.KeyVault/vaults"),
        ];
        state.view = ViewMode::ResourceTypeSummary;
        assert_eq!(
            state.type_summary(),
            [
                ("microsoft.compute/virtualmachines".to_string(), 2),
                ("microsoft.compute/disks".to_string(), 1),
                ("microsoft.keyvault/vaults".to_string(), 1),
            ],
            "types merged case-insensitively, most common first"
        );
        assert_eq!(
            state.selected_type().as_deref(),
            Some("microsoft.compute/virtualmachines")
        );

        state.resource_query = ResourceQuery::parse(["type=microsoft.compute"]).unwrap();
        assert_eq!(state.type_summary().len(), 2, "summary follows the query");

        state.view = ViewMode::ResourceList;
        let names = |s: &AppState| {
            s.filtered_resources()
                .iter()
                .map(|r| r.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&state), ["vm-a", "vm-b", "disk-a"]);
        state.filter = "disk".into();
        assert_eq!(names(&state), ["disk-a"], "query AND text filter");
        state.filter.clear();
        state.resource_query = ResourceQuery::default();
        assert_eq!(names(&state).len(), 4, "memo notices the query changed");
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
