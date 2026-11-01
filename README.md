# az9s

![az9s logo](assets/az9s-logo.jpg)

**A fast, keyboard-first terminal UI for operating Azure, built for platform engineers.**

Browse every subscription, resource group and resource from the terminal, filter
instantly, and (soon) act on resources and check Azure Policy compliance, the way k9s
works for Kubernetes.

> ⚠️ **Status: early development.** No release yet. The app can browse resources but is
> slow and read-only for now. See the [Roadmap](ROADMAP.md).

## Features

| Today | Planned |
|---|---|
| Subscriptions → resource groups → resources → detail | Instant, non-blocking UI backed by Azure Resource Graph (M1) |
| Cross-subscription views (`:rgs all`, `:res all`) | Rich detail views, JSON, open in Portal, command palette (M2) |
| `/` filtering, vim keys, `:` commands, `?` help | Safe actions: VM start/stop, restarts, tags, delete (M3) |
| Offline fallback through a local SQLite cache | Azure Policy assignments, compliance and exemptions (M4) |

## Requirements

- [Rust](https://rustup.rs) 1.75+
- [Azure CLI](https://learn.microsoft.com/cli/azure/install-azure-cli), logged in with
  `az login`
- A terminal with true color support (recommended)

## Install

```bash
git clone https://github.com/mrWiecek/az9s.git
cd az9s
cargo install --path src/apps/aztui/crates/aztui --locked
```

> The binary is still named `aztui` until the rename in milestone M0.

## Quick start

```bash
az login                                   # authenticate once
az account set --subscription "<name-or-id>"   # optional
aztui                                      # launch
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
| `:subs` | Subscriptions |
| `:rgs [all\|<sub>]` | Resource groups (all subscriptions or one) |
| `:res [all\|<sub>]` | Resources (all subscriptions or one) |

## Development

Requires [just](https://github.com/casey/just).

```bash
just build        # debug build
just app-run      # run (release mode, logs off)
just test         # run tests
just fmt          # format
just clippy       # lint (warnings are errors)
```

Debug logging: `cd src/apps/aztui && RUST_LOG=debug cargo run`.

Project layout (will be flattened in M0):

```
src/apps/aztui/crates/
├── aztui        # binary entry point
├── aztui-core   # state, navigation, SQLite cache
├── aztui-azure  # Azure auth + API client
└── aztui-tui    # ratatui UI
```

More docs: [`src/apps/aztui/docs/`](src/apps/aztui/docs/), including
[ARCHITECTURE.md](src/apps/aztui/docs/ARCHITECTURE.md).

## Contributing

Issues and PRs are welcome. Start with [ROADMAP.md](ROADMAP.md) and
[CONTRIBUTING.md](src/apps/aztui/CONTRIBUTING.md). Record user-visible changes under
`[Unreleased]` in [CHANGELOG.md](CHANGELOG.md).

## License

MIT. See [LICENSE](LICENSE). (Final licensing is an open decision in the roadmap.)
