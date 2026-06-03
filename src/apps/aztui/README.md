# aztui

A keyboard-first Azure browser for the terminal, built as a Rust workspace on Ratatui and Crossterm.

## Repository Layout

```text
src/apps/aztui/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── crates/
│   ├── aztui/
│   ├── aztui-core/
│   ├── aztui-azure/
│   └── aztui-tui/
└── docs/
```

## Build And Run

Prerequisites:

- Rust toolchain installed
- Azure CLI installed if you want live subscription data

From this directory:

```bash
cargo check
cargo run
```

With debug logging:

```bash
RUST_LOG=debug cargo run
```

Release build:

```bash
cargo build --release
./target/release/aztui
```

## Development

```bash
cargo test
cargo clippy
```

The application cache is stored under the user data directory, for example `~/.local/share/aztui/aztui.db` on Linux.

## What Exists Today

- Azure authentication through Azure CLI and default credential resolution
- Subscription-oriented TUI shell with keyboard navigation
- SQLite-backed cache layer
- Separate crates for app, core state, Azure integration, and TUI rendering

## Documentation

- `docs/ARCHITECTURE.md`
- `docs/BUILDING.md`
- `docs/AZURE_AUTHENTICATION.md`
- `docs/AZURE_SDK_INTEGRATION.md`
- `docs/DATA_LAYER.md`
- `docs/KEYBOARD_SHORTCUTS.md`
- `docs/NAVIGATION_IMPLEMENTATION_GUIDE.md`
- `docs/TUI_ARCHITECTURE.md`
- `docs/archive/` for historical generation notes and one-off fix artifacts

## Code Quality

- ✓ Zero compilation errors
- ✓ Buildable with `cargo check`
- ✓ Comprehensive documentation
- ✓ Production-ready architecture

## Contributing

This project is in active development. The workspace is checked in as the canonical app and historical generation artifacts are archived under `docs/archive/`.

## License

MIT License - See LICENSE file for details

## Acknowledgments

- Inspired by [k9s](https://k9scli.io/) - Kubernetes CLI
- Built with [Ratatui](https://ratatui.rs/) - Terminal UI framework
- Powered by [Azure SDK for Rust](https://github.com/Azure/azure-sdk-for-rust)
