# Architecture

This describes the code as it is **today**. Milestone M1 in [ROADMAP.md](../ROADMAP.md)
changes the runtime model (see "Known limitations" below). Update this file in the same
PR when that happens.

## Crates

```
crates/
├── az9s         # binary: sets up tracing, builds az9s_core::App, calls az9s_tui::run
├── az9s-core    # app state, navigation, filtering, SQLite cache
├── az9s-azure   # authentication + Azure API client
└── az9s-tui     # ratatui/crossterm UI: event loop, key handling, rendering
```

Dependencies only point downward: `az9s → az9s-tui → az9s-core → az9s-azure`.
`az9s-azure` knows nothing about the UI or the cache.

## az9s-azure

- `auth.rs`: `AzureAuth` uses `AzureCliCredential` and falls back to
  `DefaultAzureCredential`. It is exposed through the `AuthProvider` trait.
- `client.rs`: `AzureClient`.
  - `list_subscriptions` shells out to `az account list --all`.
  - `list_resource_groups` and `list_resources` call ARM through
    `azure_mgmt_resources`, one subscription per call, and page through all results.
- `resource_graph.rs`: Azure Resource Graph REST client.
  - `query_pages` POSTs KQL scoped to a management group or a subscription list
    and hands each 1000-row page to a callback, in order. After the first page
    reports `totalRecords`, the remaining pages are fetched by `$skip`, 4 at a
    time (`PARALLEL_PAGES`). If that fails, it falls back to sequential
    `$skipToken` paging without repeating rows.
  - `CachingTokenSource` reuses the bearer token until 5 minutes before its
    `exp` claim. Concurrent callers share one refresh.
  - It respects the per-user quota headers and retries 429s. Errors carry the HTTP
    status and the ARM error message (`AzureError::Http`).
  - It also holds the KQL builders and row → model mapping. The resource group
    name is taken from the resource ID to keep its casing, because Resource
    Graph lowercases the `resourceGroup` column.
- `management_groups.rs`: `ManagementGroupClient`. `descendants(group)` reads
  every management group and subscription below a group with its parent, in
  one paged call (`nextLink`); `display_name(group)` names the root.
- `timing.rs`: always-on duration stats (`timing::timer("azure.token")`),
  re-exported as `az9s_core::timing`. `az9s --timings` prints a summary on exit.
- `testing.rs` (feature `test-util`): a static token source and a local mock HTTP
  server, used by tests in this crate and in `az9s-core`.
- `models.rs`: Azure-side DTOs (`AzureSubscription`, `AzureResourceGroup`,
  `AzureResource`).

## az9s-core

- `app.rs`: `App` owns `AppState`, the `Cache` and the `Loader`.
  - `App::new()` opens the cache, shows cached subscriptions immediately, starts
    background fetches of fresh subscriptions and the management group tree,
    and opens the tree view (`App::open_scope_tree`). It never waits for Azure.
    If the tree fails with nothing cached, it falls back to the subscription
    list with a status-bar notice instead of an error popup.
  - `App::request(LoadRequest)` starts a background load and returns at once.
    If the target list is empty, it is first filled from the cache for exactly
    that scope.
  - `App::apply(LoadResult)` handles streamed chunks and the final result.
    - If cached rows were on screen when the request started, chunks are
      staged off-screen and swapped in once, keeping the selection by ID.
      Otherwise chunks append to the visible list.
    - A successful final result queues a cache write that replaces the scope.
      Subscriptions that failed to load are excluded.
    - `AppState::freshness` records each view's source and age, which the
      status bar shows.
- `loader.rs`: background loading.
  - `Loader` runs each `LoadRequest` on a tokio task and sends a `LoadResult` over
    a channel.
  - One request per `LoadKind` (subscriptions, resource groups, resources) is in
    flight at a time. A newer request aborts the older one, and stale results
    are dropped by generation number.
  - Fetchers may **stream** partial results through a `ChunkSink`. The first
    chunk of a request replaces the list, later chunks append, and a final
    message ends the request (`Accept::{Partial, Final, Stale}`).
  - `Fetcher` is the data-source trait.
    - `GraphFetcher` (the default) lists resource groups and resources through
      Resource Graph. "All subscriptions" queries go to the tenant root
      management group (ID = the token's `tid`) and are filtered to the requested
      subscriptions; if that scope is forbidden, it retries with the
      subscription list. It streams each page. If it fails before any rows
      arrive, it falls back to ARM and sets a notice.
    - With `with_management_groups`, `GraphFetcher` also serves
      `LoadRequest::ScopeTree`: the descendants of the tenant root group
      (name = tenant ID).
    - `AzureFetcher` uses ARM (one call per subscription, 16 at a time) and
      handles the subscription list (Azure CLI). Set `AZ9S_BACKEND=arm` to use it
      for everything.
    - `Fetcher::warm_up` starts one-time setup in the background at startup.
      `GraphFetcher` uses it to resolve the tenant (and so the token) before
      the first request.
    - Tests use fake fetchers.
- `detail.rs` (with type-specific sections, e.g. a VNet's subnets and their
  delegations): what the resource detail view shows, derived from the
  resource's full document (`LoadRequest::ResourceDetail`, one Resource
  Graph query by ID; cached in `resource_details`): Overview sections, sorted
  tags, pretty JSON, and Portal URLs.
- `query.rs`: `ResourceQuery`, the structured filters typed after `:res`
  (`type=`, `loc=`, `tag=`): parsing, matching, and `Display` back to the
  typed form (stored in navigation contexts so `Esc` restores it). Applied
  on the client in `AppState::filtered_resources`, with the `/` text filter,
  in one memoized pass; `AppState::type_summary` counts types and
  `AppState::tag_summary` counts tag key/value pairs (each memoized per list
  and query).
- Tenants (`:ctx`): `App::switch_tenant` swaps the loader's fetcher for one
  built by the `FetcherFactory` (tokens from `CliTenantTokenSource`, i.e.
  `az account get-access-token --tenant`), drops the old tenant's lists and
  reloads the start view. `AppState::tenant` filters subscriptions
  (`in_tenant`); the scope tree is cached per tenant (`scope_tree:<id>`).
  Names come from `ManagementGroupClient::tenants` (`GET /tenants`).
- `policy.rs`: Azure Policy model. `PolicyAssignment` and `Compliance`
  parse Resource Graph rows; `PolicyScope` (management group or
  subscription); `applicable` finds the assignments at a scope and its
  ancestors in the management group tree (`Origin::Here` / `Inherited`);
  `assigned_below` counts those further down. `App::open_policy` loads
  `LoadRequest::PolicyAssignments` (tenant-wide) and
  `LoadRequest::PolicyCompliance { scope }`, both cached as documents, and
  `LoadRequest::ComplianceSummary { scope }`: distinct resources evaluated /
  non-compliant per subscription (cached as `compliance_summary:<scope>`),
  summed by `ComplianceSummary::from_subscriptions` for the view's first
  line. `LoadRequest::ComplianceRollup` runs the same query tenant-wide
  (cached as `compliance_rollup:<tenant>`, loaded with the tree):
  `AppState::group_compliance` adds up a group's subscriptions for the
  tree's column, `compliance_rows` ranks them for `:compliance`, and
  `policy_summary` falls back to it until the scope's own summary arrives.
  It is a separate load kind so it never cancels, or is cancelled by, the
  policy view's summary.
- `rbac.rs`: `RoleAssignment`, `RoleDefinition` (built-in or custom, its
  permissions) from Resource Graph `authorizationresources`
  (`LoadRequest::RoleAssignments` / `RoleDefinitions`, tenant-wide, cached
  per tenant; without tenant-root access they query the user's
  subscriptions instead, with a notice). `assignments_at` says how each
  assignment reaches a scope (`Reach::Inherited`/`Here`/`Below`).
  Assignments name roles by an ID at their own scope, so roles are matched
  by GUID. Principal names: `LoadRequest::Principals` asks Microsoft Graph
  (`az9s_azure::directory::DirectoryClient`, `directoryObjects/getByIds`,
  a Graph-audience token from the same login) for the IDs not yet named;
  names accumulate in `AppState::principals` and are cached per tenant.
  One principal's access (`App::open_access`, `AppState::ra_principal`)
  adds its groups from `LoadRequest::PrincipalGroups` (Graph
  `transitiveMemberOf`, nested groups included) and keeps the assignments
  of the principal or any of those groups, each with the group it comes
  `via`. Navigation contexts of `:ra` carry its scope, role and principal
  so `Esc` restores the list as it was.
- Applications (`az9s_azure::apps`, `az9s_core::apps`, `app_view`):
  `LoadRequest::Applications` loads every registration and service
  principal (cached per tenant); `apps::app_rows` joins them by app ID into
  one row per app (`AppKind`). Opening one loads `AppDetail` fresh: owners,
  delegated grants, app role assignments (granted and assigned), and the
  APIs involved, to name permissions. `permission_rows` merges requested
  (registration) and granted (enterprise app) permissions with their
  consent. `app_view::app_lines` renders each `AppTab` as toned lines,
  shared by the screen and `c`.
- Groups (Microsoft Graph via `DirectoryClient`): `LoadRequest::Groups`
  (paged `/groups`, cached per tenant), `GroupMembers { transitive }`
  (`members` or `transitiveMembers`, fresh each time) and the existing
  `PrincipalGroups` for a member's groups. Views `GroupList`,
  `GroupMembers`, `MemberOf`; their navigation contexts carry whose
  members or groups they showed. `detail::entra_portal_url` opens groups,
  users and service principals on their Entra pages.
- Exemptions: `LoadRequest::PolicyExemptions` (tenant-wide, cached as
  `policy_exemptions:<tenant>`) into `PolicyExemption`. `:exemption` lists
  those `within` a scope (`AppState::exemption_rows`, by `Expiry`); the
  Policy tab finds the exemption behind an Exempt state with
  `AppState::exemption_for` (covers the state, not expired, most specific
  scope). Per-subscription rows add up exactly, so management groups can be
  rolled up from them.
  The drill-down is not cached (policy states change often):
  `App::open_non_compliant` loads `LoadRequest::PolicyStates` (one
  assignment's non-compliant states in the policy scope), shown one row per
  resource by `group_by_resource`; `App::load_resource_policy` loads
  `LoadRequest::ResourcePolicy` when the detail view's Policy tab opens,
  then names the definitions and initiatives (`name_definitions`: one
  Resource Graph lookup across the tenant, ARM `GET {id}` for those it
  doesn't list). Policy definitions opened from that tab fall back to ARM
  the same way (`ManagementGroupClient::policy_object`).
- Detail tabs with rows (Tags, Policy) keep a cursor (`detail_cursor`);
  `Enter` opens the row. Navigation contexts can carry `RESOURCES` (the
  resource list request a view showed, reloaded on Esc after a tag's
  tenant-wide list replaced it) and `DETAIL` (the resource, tab and row to
  reopen on Esc from a policy definition).
- `config.rs`: the optional config file (`Config::path`, `Config::load`,
  `TEMPLATE` for `--init-config`). `App::start` applies it: the start view,
  and `AppState::config` for the default subscription, auto-refresh
  (`TuiApp::tick_at`) and read-only mode. Invalid files fall back to the
  defaults with a status-bar notice.
- `fuzzy.rs`: fzf-style fuzzy matching (in-order characters; contiguous
  runs, word starts and early matches score higher; several words must all
  match).
- `palette.rs`: the `Ctrl+P` palette: items from commands and loaded data,
  each with a `PaletteAction`; `Palette` snapshots the items when opened
  and re-searches on each keystroke.
- `complete.rs`: `:` autocomplete: suggestions for the word being typed
  (commands, scope, filter keys, and values from loaded data) and `apply`.
- `export.rs`: `view_text`, what `c` copies: a list's filtered rows as TSV
  with a header, the tree or type summary rows, or the detail view's tab.
- `resource_type.rs`: short aliases for resource types (`vm`, `vnet`, …),
  shown in lists and matched by `type=` and `/` filters.
- `scope_tree.rs`: `ScopeTree` (root, groups, subscriptions, each with its
  parent; serializable, cached as one JSON document). `rows(expanded, known
  subscriptions, filter)` flattens it into the visible `TreeRow`s: groups
  before subscriptions, sorted by name, with subscription counts. A filter
  keeps matches plus their ancestors. Subscriptions missing from the tree go
  under an "unplaced" pseudo-group. `subscriptions_under(group)` gives the
  scope for "open this group".
- `state.rs`: `AppState` holds:
  - the current `ViewMode` (`ManagementGroupTree`, `SubscriptionList`,
    `ResourceGroupList`, `ResourceList`, `ResourceDetail`)
  - the scope tree, which groups are open (`tree_expanded`), and the group a
    cross-subscription list was opened from (`scope_group`, used by refresh)
  - the loaded lists and the selection (`ListState`)
  - the navigation stack (`NavigationContext`)
  - filter and command-mode buffers, and loading/error flags
  - a memoized filter result: `filtered_*()` rescans rows only when the filter text
    or the list itself changes. Replace lists wholesale; don't edit rows in place.
  - `DataSource` (Azure or Cache)
- `cache/`: a SQLite database at the platform data dir (`directories` crate, e.g.
  `~/.local/share/az9s/az9s.db` on Linux).
  - `migrations.rs`: versioned schema with tables `subscriptions`,
    `resource_groups`, `resources` and `resource_details` (v1), and `kv`
    for whole documents such as the scope tree (v2).
  - `repository.rs`: reads and writes, plus a `ResourceFilter` query builder.
  - `scoped.rs`: scope-based reads (`cached_resource_groups(subscription_ids)`,
    `cached_resources(subscription_ids, resource_group)`) that return the rows
    plus the oldest `cached_at`, and `replace_*` writes that replace a whole
    scope in one transaction.
    - Parent tables (`subscriptions`, `resources`) are written with upserts,
      never `INSERT OR REPLACE`: SQLite runs REPLACE as delete + insert, which
      fires `ON DELETE CASCADE` and wipes the children.
    - Child rows whose subscription isn't cached are skipped.
  - `writer.rs`: file-backed caches send `WriteJob`s to a background thread
    with its own connection (WAL mode), so large writes never block the UI.
    `Cache::flush` waits for queued writes. In-memory caches (tests) write
    synchronously.
  - `warming.rs`: cache-warming strategies. **Not yet used by the app.**
- `App::offline(cache)` builds an app with no Azure connection. Tests and benchmarks
  use it.
- `models.rs`: core domain models (`Subscription`, `ResourceGroup`, `Resource`,
  `ResourceDetail`).

## az9s-tui

- `lib.rs`: terminal setup and teardown (raw mode, alternate screen, panic hook),
  plus the main loop: draw → wait for terminal input **or** a load result
  (`tokio::select!`) → handle it and everything else already queued → draw. A
  burst of key repeats costs one redraw.
- `event.rs`: a dedicated input thread blocks on crossterm and forwards every event
  as soon as it arrives. It sends `Tick` after 100 ms without input, which drives
  the loading spinner.
- `app.rs`: `TuiApp` wraps `App`. It handles keys per mode (normal, filter, command,
  help) and moves between views. Opening a view switches to it immediately and
  calls `App::request`; the list fills in when the result arrives.
- `ui/search.rs`: Vim-style search for the detail view: smartcase matching,
  next/previous with wrap-around, and highlighting that splits styled spans.
  It runs over `ui::detail_lines`, the same lines the view draws, so match
  positions and the screen agree.
- `ui/theme.rs`: colours by role (`theme().accent`, `.muted`,
  `.selection()`, `.search_match()`), one `Theme` per `theme` setting;
  set once at startup from the config (`NO_COLOR` forces mono). Views never
  name a colour directly.
- `desktop.rs`: the `Desktop` trait for copying (OSC 52 escape sequence)
  and opening URLs (`open` / `xdg-open` / `start`). Tests record instead.
- `command.rs`: parses `:` commands (`:mg`, `:sub`, `:rg [all|<sub>]`,
  `:type`, filters after `:res`/`:type`,
  `:res [all|<sub>]`). Command names are always singular.
- `ui/`: rendering. List views are **virtualized**: `ListState::visible_range`
  picks the rows that fit the viewport, only those become widgets, and the scroll
  offset is written back to state. Lists are in `mod.rs`, with `breadcrumb.rs`, `status_bar.rs`,
  `help.rs`, `resource_type.rs` icons and short names, `widgets.rs`).

## Data flow (current)

```
key press → input thread ─┐
                          ├─ run_loop (select!, drains queues) → draw
load task (tokio) ────────┘
   ▲                         │
   └── App::request ◄── TuiApp::handle_key       App::apply ◄── LoadResult
```

## Known limitations (addressed in M1)

1. ~~The event task forwards at most one terminal event per 250 ms tick.~~ Fixed.
2. ~~Azure calls are awaited inside the key handler, so the UI froze.~~ Fixed
   (background `Loader`).
3. ~~Opening a resource group fetched every resource in the subscription.~~ Fixed
   (ARM `list_by_resource_group`).
4. ~~"All subscriptions" views made one ARM call per subscription.~~ Fixed: one
   Resource Graph query at the tenant root, streamed page by page.
5. ~~The cache was only read at startup for subscriptions.~~ Fixed: every
   list renders cached data first and refreshes in the background.

The M1 target design:
- Background workers send results to the UI over a channel.
- Lists come from Azure Resource Graph with KQL filters across all subscriptions.
- The UI renders cached data first and refreshes it in the background.
