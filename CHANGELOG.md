# Changelog

All notable changes to az9s are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Planned work is tracked in [ROADMAP.md](ROADMAP.md), not here.

## [Unreleased]

No version has been released yet. The first public release is planned as **v0.1.0**
(see milestone M1 in the roadmap).

### Added
- `ROADMAP.md` with milestones M0–M5 and recorded product decisions.
- GitHub Actions CI: `fmt`, `clippy -D warnings`, and tests on Linux, macOS and
  Windows.
- `just ci`, `just install` and `just run-debug` (logs to `az9s.log`).
- `CONTRIBUTING.md`, `docs/ARCHITECTURE.md` (describes the current code), PR and
  issue templates, `CODEOWNERS`, `rustfmt.toml`.

### Changed
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
- Cross-subscription views: `:rgs all` and `:res all`.
- Vim-style navigation (`j/k`, `g/G`, `Ctrl+D/U`), `/` filtering, `:` commands, and
  `?` help.
- A SQLite cache, used as a fallback when Azure is unreachable at startup.
- A Rust workspace with 4 crates and 65 passing tests.

### Known issues
- The UI is slow and freezes during Azure calls. "All subscriptions" views make one
  sequential call per subscription, which takes minutes at 400+ subscriptions (fix
  planned in milestone M1).
- Opening a resource group lists every resource in its subscription instead of only
  that group's resources (fix planned in milestone M1).
