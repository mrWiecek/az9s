# az9s

![az9s logo](assets/az9s-logo.jpg)

**A fast, keyboard-first terminal UI for operating Azure, built for platform engineers.**

Browse every subscription, resource group and resource from the terminal, filter
instantly, and (soon) act on resources and check Azure Policy compliance, the way k9s
works for Kubernetes.

> ⚠️ **Status: early development.** No release yet. The app browses resources across
> all your subscriptions with Azure Resource Graph and never freezes while loading.
> It is read-only for now. See the [Roadmap](ROADMAP.md).

## Features

| Today | Planned |
|---|---|
| Subscriptions → resource groups → resources → detail | Instant, non-blocking UI backed by Azure Resource Graph (M1) |
| Cross-subscription views (`:rg all`, `:res all`) | Management-group tree, rich detail views, JSON, open in Portal, command palette (M2) |
| `/` filtering, vim keys, `:` commands, `?` help | Azure Policy at management-group scope: assignments, compliance, exemptions (M3) |
| Offline fallback through a local SQLite cache | Safe actions: VM start/stop, restarts, tags, delete, policy scans (M4) |

## Requirements

- [Rust](https://rustup.rs) (stable) and a C compiler (`build-essential` on
  Debian/Ubuntu/WSL; Xcode command line tools on macOS). OpenSSL and `pkg-config`
  are **not** needed: TLS uses rustls.
- [Azure CLI](https://learn.microsoft.com/cli/azure/install-azure-cli), logged in with
  `az login`
- A terminal with true color support (recommended)

## Install

```bash
git clone https://github.com/mrWiecek/az9s.git
cd az9s
cargo install --path crates/az9s --locked   # or: just install
```

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
| `Enter` | Open |
| `Esc` / `Backspace` | Back, clear filter, or dismiss error |
| `/` | Filter |
| `:` | Command mode |
| `r` | Refresh |
| `?` | Help |
| `q` / `Ctrl+C` | Quit |

## Commands

| Command | Shows |
|---|---|
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
| Status bar says "Resource Graph unavailable … used ARM" | Lists still load, but more slowly; check `just run-debug` logs |
| A resource you just changed looks stale | Resource Graph can lag ARM briefly; press `r`, or run with `AZ9S_BACKEND=arm` |
| Something feels slow | Run `az9s --timings` and include the table in an issue |

## Contributing

Issues and PRs are welcome. Start with [ROADMAP.md](ROADMAP.md) and
[CONTRIBUTING.md](CONTRIBUTING.md). Record user-visible changes under
`[Unreleased]` in [CHANGELOG.md](CHANGELOG.md).

## License

[Apache License 2.0](LICENSE).
