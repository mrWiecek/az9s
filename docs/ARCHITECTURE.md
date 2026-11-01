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
- `models.rs`: Azure-side DTOs (`AzureSubscription`, `AzureResourceGroup`,
  `AzureResource`).

## az9s-core

- `app.rs`: `App` owns `AppState`, the `Cache` and the `Loader`.
  - `App::new()` opens the cache, shows cached subscriptions immediately, and
    starts a background fetch of fresh ones. It never waits for Azure.
  - `App::request(LoadRequest)` starts a background load and returns at once.
    `App::apply(LoadResult)` stores a finished load in state and the cache.
- `loader.rs`: background loading.
  - `Loader` runs each `LoadRequest` on a tokio task and sends a `LoadResult` over
    a channel.
  - One request per `LoadKind` (subscriptions, resource groups, resources) is in
    flight at a time. A newer request aborts the older one, and stale results
    are dropped by generation number.
  - `Fetcher` is the data-source trait. `AzureFetcher` calls ARM, and
    multi-subscription loads run up to 16 at a time (`fetch_per_subscription`).
    Tests use fake fetchers. Resource Graph will plug in here (M1 step 3).
- `state.rs`: `AppState` holds:
  - the current `ViewMode` (`SubscriptionList`, `ResourceGroupList`, `ResourceList`,
    `ResourceDetail`)
  - the loaded lists and the selection (`ListState`)
  - the navigation stack (`NavigationContext`)
  - filter and command-mode buffers, and loading/error flags
  - a memoized filter result: `filtered_*()` rescans rows only when the filter text
    or the list itself changes. Replace lists wholesale; don't edit rows in place.
  - `DataSource` (Azure or Cache)
- `cache/`: a SQLite database at the platform data dir (`directories` crate, e.g.
  `~/.local/share/az9s/az9s.db` on Linux).
  - `migrations.rs`: versioned schema with tables `subscriptions`,
    `resource_groups`, `resources` and `resource_details`.
  - `repository.rs`: reads and writes, plus a `ResourceFilter` query builder.
  - `warming.rs`: cache-warming strategies. **Not yet used by the app.**
- `timing.rs`: always-on duration stats (`timing::timer("azure.list_resources")`).
  `az9s --timings` prints a summary on exit. Recorded operations include
  `startup.first_frame`, `ui.frame`, `ui.handle_key`, `azure.*` and `cache.*`.
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
- `command.rs`: parses `:` commands (`:sub`, `:rg [all|<sub>]`,
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
4. "All subscriptions" views still make one ARM call per subscription, now 16 at a
   time instead of sequentially. Resource Graph (step 3) replaces this with one
   query.
5. The cache is read at startup for subscriptions only. It is not used to render first and
   refresh afterwards.

The M1 target design:
- Background workers send results to the UI over a channel.
- Lists come from Azure Resource Graph with KQL filters across all subscriptions.
- The UI renders cached data first and refreshes it in the background.
