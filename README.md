# az9s

![az9s logo](assets/az9s-logo.jpg)

**A fast, keyboard-first terminal UI for operating Azure, built for platform engineers.**

Browse every subscription, resource group and resource from the terminal, filter
instantly, and (soon) act on resources and check Azure Policy compliance, the way k9s
works for Kubernetes.

> ⚠️ **Status: early development.** The app browses resources across
> all your subscriptions with Azure Resource Graph and never freezes while loading.
> It is read-only for now. See the [Roadmap](ROADMAP.md).

## Features

| Today | Planned |
|---|---|
| Management group tree → subscriptions → resource groups → resources → detail | Rich detail views, JSON, open in Portal, command palette (M2) |
| Resource groups across a whole management group, or all subscriptions (`:rg all`, `:res all`) via Azure Resource Graph | Azure Policy at management-group scope: assignments, compliance, exemptions (M3) |
| Instant views from a local cache, refreshed in the background, with data age in the status bar | Safe actions: VM start/stop, restarts, tags, delete, policy scans (M4) |
| `/` filtering, vim keys, `:` commands, `?` help | AKS, Key Vault, logs, themes (M5) |

## Requirements

- [Rust](https://rustup.rs) 1.88 or newer and a C compiler (`build-essential` on
  Debian/Ubuntu/WSL; Xcode command line tools on macOS). OpenSSL and `pkg-config`
  are **not** needed: TLS uses rustls.
- [Azure CLI](https://learn.microsoft.com/cli/azure/install-azure-cli), logged in with
  `az login`
- A terminal with true color support (recommended)

## Install

Download a prebuilt binary for Linux, macOS or Windows from
[Releases](https://github.com/mrWiecek/az9s/releases), or build from source:

```bash
cargo install --git https://github.com/mrWiecek/az9s --locked az9s              # latest
cargo install --git https://github.com/mrWiecek/az9s --tag v0.3.0 --locked az9s # a specific version
```

`az9s --version` shows the installed version and commit. Versions follow
SemVer; see [CHANGELOG.md](CHANGELOG.md) for what changed and
[RELEASING.md](RELEASING.md) for how to roll back.

## Quick start

```bash
az login                                   # authenticate once (az9s uses your Azure CLI login)
az9s                                       # launch
az9s --timings                             # launch, print a timing summary on exit
```

## Keys

| Key | Action |
|---|---|
| `j` / `k`, `↑` / `↓` | Move down / up |
| `g` / `G` | First / last item |
| `Ctrl+D` / `Ctrl+U` | Page down / up |
| `Enter` | Open (in the tree: resource groups of the group or subscription) |
| `Space`, `l`/`→`, `h`/`←` | Tree: open/close a group (`h` on a closed row goes to the parent) |
| `Esc` / `Backspace` | Back, clear filter, or dismiss error |
| `/` | Filter |
| `:` | Command mode |
| `r` | Refresh |
| `?` | Help |
| `q` / `Ctrl+C` | Quit |

## Commands

| Command | Shows |
|---|---|
| `:mg` | Management group tree (the start view) |
| `:sub` | Subscriptions |
| `:rg [all\|<sub>]` | Resource groups (all subscriptions or one) |
| `:res [all\|<sub>]` | Resources (all subscriptions or one) |

## Development

Requires [just](https://github.com/casey/just).

```bash
just run         # run (release mode, logs off)
just run-debug   # run with debug logs written to az9s.log
just run-timings # run, then print Azure/cache/frame timings on exit
just perf        # rendering benchmark with 100k rows
just ci          # everything CI runs: fmt check, clippy, tests
just --list      # all recipes
```

```
crates/
├── az9s         # binary entry point
├── az9s-core    # state, navigation, SQLite cache
├── az9s-azure   # Azure auth + API client
└── az9s-tui     # ratatui UI
```

How it fits together: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Troubleshooting

| Symptom | Try |
|---|---|
| "Not connected to Azure" | Run `az login`, then press `r` |
| "Management group tree unavailable" | You need read access on the tenant root management group; `:sub` and `:rg all` still work |
| Status bar says "Resource Graph unavailable … used ARM" | Lists still load, but more slowly; check `just run-debug` logs |
| A resource you just changed looks stale | Resource Graph can lag ARM briefly; press `r`, or run with `AZ9S_BACKEND=arm` |
| Something feels slow | Run `az9s --timings` and include the table in an issue |

## Contributing

Issues and PRs are welcome. Start with [ROADMAP.md](ROADMAP.md) and
[CONTRIBUTING.md](CONTRIBUTING.md). Record user-visible changes under
`[Unreleased]` in [CHANGELOG.md](CHANGELOG.md), and bump the version for code
changes (`just bump patch|minor|major`; see [RELEASING.md](RELEASING.md)).

## License

[Apache License 2.0](LICENSE).
