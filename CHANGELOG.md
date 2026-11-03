# Changelog

All notable changes to az9s are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Planned work is tracked in [ROADMAP.md](ROADMAP.md), not here.

Every version below has a git tag (`vX.Y.Z`) and a
[GitHub Release](https://github.com/mrWiecek/az9s/releases) with binaries.
Release process, audit and rollback: [RELEASING.md](RELEASING.md).

## [Unreleased]

## [0.7.0] - 2026-11-03

### Added
- **Tags view: `:tag [all|<sub>] [filters]`.** Lists every tag on the
  resources in scope as key, value and how many resources carry it, most
  common first. Keys are grouped ignoring case, as Azure treats them. `/`
  narrows by key or value, `Enter` opens the resources with that tag
  (without reloading), `Esc` comes back, `c` copies the list. Filters work
  as for `:res`, e.g. `:tag all type=keyvault`.
- **Vim-style search in the resource detail view.** `/` searches the open
  tab (Overview, Tags or JSON) instead of filtering: it jumps to the first
  match as you type and highlights every match, the current one more
  strongly. `Enter` keeps the search, `Esc` cancels it and returns to where
  you were. `n` / `N` go to the next / previous match and wrap around
  ("search hit BOTTOM, continuing at TOP"); the status bar shows "match 3
  of 12". Lowercase ignores case, a capital letter makes it exact
  (smartcase). `Esc` clears the highlights, then goes back.

## [0.6.0] - 2026-11-02

### Added
- **`c` copies the current view**, in every view:
  - lists (subscriptions, resource groups, resources): every row that passes
    the filters, not only the rows on screen, tab-separated with a header,
    so it pastes into a spreadsheet;
  - the resource detail view: the current tab (Overview, Tags or JSON);
  - the management group tree and the `:type` summary: their rows.
  The status bar says what was copied and how much.
- **VNet subnets in the detail view.** A virtual network's Overview lists
  each subnet with its prefixes, delegations (e.g. `delegated to
  Microsoft.Web/serverFarms`, or `no delegation`), NSG, route table and NAT
  gateway.
- **`:rg` takes filters too:** `loc=` and `tag=`, e.g. `:rg all
  loc=northeurope` or `:rg tag=owner`. `type=` there is an error that
  points to `:res` and `:type`. Active filters show in every list's title.
- **Type aliases in filters.** `type=vnet`, `type=vm`, `type=key-vault`
  (or `keyvault`) work, as does a provider without `Microsoft.`
  (`type=network`). The `/` filter in resource lists and in `:type` matches
  aliases too (typing `vnet` in `:type` finds
  `microsoft.network/virtualnetworks`). Before, `type=` needed the type's
  full name or its last part.
- **`tag=environment=prod`** works as well as `tag=environment:prod`.
- **A rewritten, scrollable help (`?`)**: commands, the scope rules,
  every filter with its matching rules, worked examples, and all keys.
  `j`/`k`, `g`/`G` and `Ctrl+D`/`Ctrl+U` scroll it.

### Changed
- List columns no longer overlap: long names are cut with "…" leaving a
  gap before the next column, and the name column widens with the terminal.

### Fixed
- **`o` failed on WSL** with an OS error and a cut-off link: az9s ran
  `xdg-open`, which WSL usually doesn't have. On WSL it now opens the
  Windows browser (`wslview`, then `cmd.exe /c start`, then `xdg-open`).
  If no browser can be started anywhere, the Portal link is copied to the
  clipboard instead, with a short message, rather than printed into the
  status bar where it was truncated.
- **`:rg`, `:res` and `:type` without a scope always opened all
  subscriptions.** Without a scope they now apply to where you are: the
  highlighted subscription (subscription list, tree), the highlighted
  management group (tree), or the subscription or management group the
  current list is scoped to. Only with no such context, or with `all`, do
  they cover all subscriptions.

## [0.5.0] - 2026-11-02

### Added
- **Resource filters:** `:res [all|<sub>] type=… loc=… tag=…`.
  - `type=` matches the full type (`Microsoft.Compute/virtualMachines`), its
    last part (`virtualMachines`) or a provider (`Microsoft.Compute`).
  - `loc=` matches a location; `tag=env:prod` a tag value, `tag=env` any
    resource with that tag.
  - Different filters must all match; comma-separated values are
    alternatives (`type=virtualMachines,disks`). Case doesn't matter.
  - Filters apply to the loaded list on your machine, so they are instant
    (cached rows included), combine with `/`, show in the list title, and
    survive `r` and going back. A new `:res` without filters clears them.
  - An unknown filter (e.g. `size=big`) is an error that names the valid
    ones.
- **Resource type summary:** `:type [all|<sub>] [filters]` lists how many
  resources of each type there are, most common first. `Enter` on a type
  opens the resource list filtered to it, without reloading; `Esc` returns.
- The `perf` CI job also checks filtered lists and the type summary at 100k
  resources (each frame under 1 ms on CI hardware today; the budget is 16 ms).

## [0.4.0] - 2026-11-02

### Added
- **Resource detail view with Overview, Tags and JSON tabs.** `Enter` on a
  resource opens it at once with what the list knows, then loads the full
  resource document (one Resource Graph query) in the background.
  - Overview: name, type, kind, location, zones, resource group,
    subscription, provisioning state, managed by, ID, plus SKU, managed
    identity, plan, and the values in `properties` as dotted names (e.g.
    `hardwareProfile.vmSize`, up to 60 rows).
  - Tags: sorted by key. JSON: the whole document, colour-highlighted.
  - `Tab`/`Shift+Tab` (or `l`/`h`, `1`-`3`) switch tabs; `j`/`k`, `g`/`G`,
    `Ctrl+D`/`Ctrl+U` scroll; `r` reloads.
  - Documents are cached: reopening a resource shows it instantly, then
    refreshes. If loading fails, the basic view stays with a status-bar
    notice instead of an error popup.
- **`y` copies the selected item's Azure ID** (subscription, management
  group, resource group or resource) in every view. It uses the terminal's
  clipboard support (OSC 52), so it also works over SSH; in tmux, enable
  `set-clipboard`.
- **`o` opens the selected item in the Azure Portal**, in the right tenant.
  If no browser can be started, the status bar shows the link.
- `azure.resource_detail` in `--timings`.

### Changed
- Refreshing a resource list now deletes only the cached resources that are
  gone from Azure, instead of rewriting the whole scope, so cached resource
  details survive a refresh.

## [0.3.1] - 2026-11-02

### Fixed
- **Starting two az9s processes at once could crash with "UNIQUE constraint
  failed: schema_migrations.version"** (#9), e.g. right after upgrading to
  0.3.0, whose cache format change (migration v2) both tried to apply.
  Migrations now run in one transaction that takes the database's write
  lock first; the other process waits and finds nothing left to do.
- Two processes creating a new cache file at the same moment could fail with
  "database is locked" while switching it to WAL mode. az9s now retries.

## [0.3.0] - 2026-11-02

### Added
- **Management group tree, the new start view** (`:mg`). az9s opens at the
  tenant root management group and shows the hierarchy down to subscriptions,
  with the number of subscriptions below each group.
  - `Space` opens or closes a group; `l`/`→` opens, `h`/`←` closes (on a
    closed row it jumps to the parent).
  - `Enter` on a group lists the resource groups of every enabled
    subscription below it, in one Resource Graph query. `r` refreshes that
    same scope, and `Esc` returns to the tree with the group still selected.
    `Enter` on a subscription opens its resource groups.
  - `/` filters the tree; matches are shown with their parent groups opened.
  - Subscriptions you can see but that are not in the tree (e.g. from another
    tenant) are listed under "Not in the management group tree".
  - The tree is cached and shown instantly on the next start, then refreshed
    in the background (one call to the management group descendants API).
  - If the tree can't be loaded (no read access on the tenant root, or
    `AZ9S_BACKEND=arm`) and nothing is cached, az9s starts on the
    subscription list and says why in the status bar.
- Going back (`Esc`) now restores the selected row in every list, not just the
  first row.
- `azure.management_groups` in `--timings`.

### Fixed
- A long subscription name with non-ASCII characters could crash the header
  breadcrumb (byte-index string slicing).

## [0.2.2] - 2026-11-02

### Fixed
- The declared minimum Rust version was 1.75, but dependencies require
  **1.88** (`time 0.3.47`). `rust-version` and the README now say 1.88, and a
  new `msrv` CI job builds with exactly that version.

### Changed
- CI runs the 100k-row rendering benchmark (`perf` job) and fails if a frame
  takes 16 ms or more.

## [0.2.1] - 2026-11-02

### Changed
- **Faster complete loads for large scopes** (#9). Resource Graph results come
  in 1000-row pages, previously fetched one after another (about 0.6 s each).
  After the first page reports the total, the remaining pages are now fetched
  4 at a time by offset and still delivered in order. If offset paging fails,
  az9s continues page by page without repeating rows.
- **The Azure token is fetched once and reused** until shortly before it
  expires, instead of once per page. Concurrent requests share one fetch.
- **The token and tenant are fetched in the background at startup**, so the
  first "all subscriptions" view doesn't wait about 0.8 s for the Azure CLI.
- New `azure.token`, `azure.graph_page` and `azure.warm_up` entries in
  `--timings`.

### Fixed
- The tenant used for "all subscriptions" queries was looked up again on every
  request. Its cache was cloned per request (a cloned `OnceCell` is a copy), so
  each `:rg all` / `:res all` paid an extra Azure CLI token call.

## [0.2.0] - 2026-11-02

### Added
- **Cache-first views.** Opening a resource group or resource list (one
  subscription, one resource group, or `:rg all` / `:res all`) shows the cached
  rows for exactly that scope immediately, then refreshes from Azure in the
  background.
  - When cached rows are on screen, fresh pages are collected off-screen and
    swapped in once, so the list doesn't shrink to the first page. The
    selection stays on the same item.
  - With nothing cached, pages stream in as before.
- **Data age in the status bar** for the current view, e.g. `● Azure · just now`
  or `◐ Cached · 3h ago`.

### Changed
- When Azure can't be reached but cached rows are on screen, az9s shows a
  status-bar warning instead of an error popup.
- Cache writes run on a background thread, and the database uses WAL mode.
  Writing 20k resources takes about 0.5 s, which used to block the UI.
- A completed refresh replaces the cached scope as a whole, so resources and
  resource groups deleted in Azure disappear from the cache. Subscriptions that
  failed to load keep their cached rows.

### Fixed
- **Every start of az9s wiped the cached resource groups and resources.**
  Refreshing subscriptions used `INSERT OR REPLACE`, which SQLite runs as
  delete + insert, and the schema's `ON DELETE CASCADE` then deleted every
  child row. All cache writes to parent tables now use upserts.
- A long subscription name with non-ASCII characters could crash the status
  bar (byte-index string slicing).

## [0.1.0] - 2026-11-02

First tagged release.

### Added
- **Versioning and releases.** Plain SemVer: patch for fixes, minor for features,
  major for breaking changes. Merging a version bump to `main` tags it and
  publishes a GitHub Release with binaries for Linux, macOS (Apple Silicon and
  Intel) and Windows, SHA-256 checksums, and that version's notes from this file.
  `just bump patch|minor|major` prepares a release, and CI fails a PR that
  changes code without bumping an already-released version. See `RELEASING.md`
  for auditing and rolling back.
- `az9s --version` shows the git commit the binary was built from.
- `ROADMAP.md` with milestones M0–M5 and recorded product decisions.
- GitHub Actions CI: `fmt`, `clippy -D warnings`, and tests on Linux, macOS and
  Windows.
- `just ci`, `just install` and `just run-debug` (logs to `az9s.log`).
- `CONTRIBUTING.md`, `docs/ARCHITECTURE.md` (describes the current code), PR and
  issue templates, `CODEOWNERS`, `rustfmt.toml`.

### Changed
- `az9s --help` and `--version` are now available; unknown arguments are rejected.
- **Renamed to az9s.** The binary is now `az9s` (was `aztui`). Crates are `az9s`,
  `az9s-core`, `az9s-azure` and `az9s-tui`. The local data dir is now `az9s`. The old
  `aztui` cache is not migrated; it is only a cache and is rebuilt on first run.
- **Flattened the repository.** The Cargo workspace moved from `src/apps/aztui/` to
  the repo root.
- **License is now Apache-2.0 only** (was MIT in `LICENSE`, "MIT OR Apache-2.0" in
  `Cargo.toml`).
- The cache location comes from the `directories` crate, so it is correct on Windows
  and macOS too.
- Logging is off unless `RUST_LOG` is set, and goes to stderr so it no longer
  corrupts the TUI.
- README rewritten for platform engineers.
- Ran `cargo fmt` across the codebase.
- **Commands are now singular:** `:sub` (was `:subs`), `:rg` (was `:rgs`),
  `:res` / `:resource` (was `:resources`). Plural forms are no longer accepted.

### Performance (M1, step 1)
- **Input is no longer rate-limited.** Previously at most one key was handled per
  250 ms tick. Now every key is handled as it arrives, and queued keys are processed
  together before one redraw.
- **List rendering is virtualized.** Only the visible rows are built each frame,
  filtering no longer allocates, and filter results are memoized. With 100k
  resources, a frame dropped from 78 ms to 0.4 ms (101 ms to 0.6 ms with an active
  filter).
- New `--timings` flag prints Azure, cache, frame and keypress timings on exit.
  New `just run-timings` and `just perf` recipes.

### Performance (M1, step 2)
- **Azure calls run in the background; the UI never freezes.** Opening a view
  switches to it immediately, with an animated spinner in the status bar, and the
  list fills in when Azure answers. You can keep navigating, go back, or open
  something else. A superseded request is cancelled and its result discarded.
- **Startup no longer waits for Azure.** Cached subscriptions are shown at once and
  refreshed in the background.
- **"All subscriptions" views (`:rg all`, `:res all`) load 16 subscriptions at a
  time** instead of one after another, and skip disabled subscriptions. If some
  subscriptions fail, the rest still show, with a warning in the status bar.

### Performance (M1, step 3)
- **Resource Graph backend.** Resource group and resource lists now come from Azure
  Resource Graph instead of one ARM call per subscription.
  - `:rg all` and `:res all` run one query at the tenant root management group,
    limited to your enabled subscriptions. If you can't read the root, az9s
    retries scoped to your subscriptions.
  - Results stream in page by page (1000 rows per page), so the first rows show
    before the whole list has loaded.
  - Opening a resource group runs a filtered query in that subscription.
  - Resource Graph throttling is respected: az9s waits out the quota and retries
    429s.
  - If Resource Graph fails before returning any rows, az9s falls back to ARM and
    says so in the status bar. `AZ9S_BACKEND=arm` forces the old ARM path.
  - Trade-off: Resource Graph can lag behind ARM by a short time after changes.

### Fixed
- **Installing no longer requires system OpenSSL or `pkg-config`** (#4). TLS now
  uses rustls, with the bundled Mozilla roots plus the OS certificate store, so
  corporate TLS-inspecting proxies keep working. The unused `azure_mgmt_compute`
  and `azure_mgmt_network` dependencies were removed. CI fails if OpenSSL
  reappears in the dependency tree.
- Resource types from Resource Graph come in lowercase. Short type labels and
  colours are now matched case-insensitively.
- Opening a resource group showed every resource in its subscription. It now
  requests only that group's resources from Azure.
- In `:rg all`, opening a resource group used the wrong subscription. It now uses
  the group's own subscription.
- Going back to an "all subscriptions" list lost its scope, so `r` refreshed
  nothing.
- Scrolling up in a long list no longer pins the selection to the bottom row. The
  scroll offset is now kept between frames.
- A resource name longer than 35 characters containing non-ASCII characters could
  crash the app (byte-index string slicing).
- On Windows, key release events are ignored so each key press acts once.

### Removed
- `src/apps/aztui/docs/`: about 80 archived iteration reports and fix scripts, plus
  about 8k lines of outdated design docs. They remain in git history.
- The non-functional `Ctrl+P` "command palette" toggle. A real palette is planned for
  M2.
- An unused Azure SDK client that was built on every subscription fetch.
- The old `src/apps/aztui/CHANGELOG.md`, which described an unreleased "0.1.0" with
  unverified claims.

### Existing functionality (development baseline)
Already in the codebase before this changelog was started:
- Azure CLI authentication, falling back to the default credential chain.
- Browsing subscriptions → resource groups → resources → a basic resource detail view.
- Cross-subscription views (now `:rg all` and `:res all`).
- Vim-style navigation (`j/k`, `g/G`, `Ctrl+D/U`), `/` filtering, `:` commands, and
  `?` help.
- A SQLite cache, used as a fallback when Azure is unreachable at startup.
- A Rust workspace with 4 crates and 65 passing tests.

### Known issues
- The Resource Graph backend is tested against a mock API but not yet against a
  live 400+ subscription tenant.
- Resource groups and resources are not yet shown from the cache before Azure
  answers (M1 step 4).

[Unreleased]: https://github.com/mrWiecek/az9s/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/mrWiecek/az9s/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/mrWiecek/az9s/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/mrWiecek/az9s/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/mrWiecek/az9s/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/mrWiecek/az9s/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/mrWiecek/az9s/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/mrWiecek/az9s/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/mrWiecek/az9s/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/mrWiecek/az9s/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/mrWiecek/az9s/releases/tag/v0.1.0
