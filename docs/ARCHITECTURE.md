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

- `app.rs`: `App` owns `AppState`, the `Cache` and an optional `AzureClient`.
  - `App::new()` authenticates and loads subscriptions **before the first frame**.
    If Azure is unreachable, it falls back to cached subscriptions.
  - `load_resource_groups` and `load_resources` fetch from Azure, write to the cache
    and replace the lists in state.
- `state.rs`: `AppState` holds:
  - the current `ViewMode` (`SubscriptionList`, `ResourceGroupList`, `ResourceList`,
    `ResourceDetail`)
  - the loaded lists and the selection (`ListState`)
  - the navigation stack (`NavigationContext`)
  - filter and command-mode buffers, and loading/error flags
  - `DataSource` (Azure or Cache)
- `cache/`: a SQLite database at the platform data dir (`directories` crate, e.g.
  `~/.local/share/az9s/az9s.db` on Linux).
  - `migrations.rs`: versioned schema with tables `subscriptions`,
    `resource_groups`, `resources` and `resource_details`.
  - `repository.rs`: reads and writes, plus a `ResourceFilter` query builder.
  - `warming.rs`: cache-warming strategies. **Not yet used by the app.**
- `models.rs`: core domain models (`Subscription`, `ResourceGroup`, `Resource`,
  `ResourceDetail`).

## az9s-tui

- `lib.rs`: terminal setup and teardown (raw mode, alternate screen, panic hook),
  plus the main loop: draw → wait for event → `handle_event`.
- `event.rs`: a background task that polls crossterm on a 250 ms tick and forwards
  `Key`, `Mouse`, `Resize` and `Tick` events over a channel.
- `app.rs`: `TuiApp` wraps `App`. It handles keys per mode (normal, filter, command,
  help), moves between views, and calls `App` loaders directly.
- `command.rs`: parses `:` commands (`:subs`, `:rgs [all|<sub>]`,
  `:res [all|<sub>]`).
- `ui/`: rendering (lists and detail in `mod.rs`, `breadcrumb.rs`, `status_bar.rs`,
  `help.rs`, `resource_type.rs` icons and short names, `widgets.rs`).

## Data flow (current)

```
key press → event task (250 ms tick) → channel → TuiApp::handle_key
          → App::load_* (await Azure, UI blocked) → AppState updated → next draw
```

## Known limitations (addressed in M1)

1. The event task forwards at most one terminal event per 250 ms tick.
2. Azure calls are awaited inside the key handler, so the UI is frozen while they run
   and the loading state is never drawn.
3. Opening a resource group fetches every resource in the subscription. Nothing
   filters on the server.
4. "All subscriptions" views query subscriptions one after another.
5. The cache is only read as a startup fallback. It is not used to render first and
   refresh afterwards.

The M1 target design:
- Background workers send results to the UI over a channel.
- Lists come from Azure Resource Graph with KQL filters across all subscriptions.
- The UI renders cached data first and refreshes it in the background.
