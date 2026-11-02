# Changelog

All notable changes to az9s are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Planned work is tracked in [ROADMAP.md](ROADMAP.md), not here.

Every version below has a git tag (`vX.Y.Z`) and a
[GitHub Release](https://github.com/mrWiecek/az9s/releases) with binaries.
Release process, audit and rollback: [RELEASING.md](RELEASING.md).

## [Unreleased]

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

[Unreleased]: https://github.com/mrWiecek/az9s/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/mrWiecek/az9s/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/mrWiecek/az9s/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/mrWiecek/az9s/releases/tag/v0.1.0
