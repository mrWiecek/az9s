# Changelog

All notable changes to az9s are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Planned work is tracked in [ROADMAP.md](ROADMAP.md), not here.

## [Unreleased]

No version has been released yet. The first public release is planned as **v0.1.0**
(see milestone M1 in the roadmap).

### Added
- `ROADMAP.md` with milestones M0–M5.
- This changelog, rewritten to describe only what exists.

### Removed
- `src/apps/aztui/CHANGELOG.md`, which described an unreleased "0.1.0" and listed
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
- The UI is slow and freezes during Azure calls (fix planned in milestone M1).
- The binary and crates are still named `aztui` (rename planned in milestone M0).
