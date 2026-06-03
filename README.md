# az9s

![az9s logo](assets/az9s-logo.jpg)

az9s is a keyboard-first Azure browser for the terminal.

This repository contains only the app codebase.

## Quick Product View

| Component | Purpose | Location |
|---|---|---|
| az9s app | Terminal Azure browser | `src/apps/aztui/` |

## Install az9s

From repository root:

```bash
cargo install --path src/apps/aztui/crates/aztui --locked
```

Note: the installed binary is currently named `aztui`.

If you prefer not to install globally:

```bash
cd src/apps/aztui
RUST_LOG=off cargo run --release --quiet
```

## Use az9s

1. Authenticate to Azure:

```bash
az login
```

2. Optional: pick a subscription:

```bash
az account set --subscription "<subscription-name-or-id>"
```

3. Launch the app:

```bash
aztui
```

## Common Run Modes

```bash
# run from source (recommended)
cd src/apps/aztui
cargo run --release --quiet

# run with debug logs
RUST_LOG=debug cargo run

# build release
cargo build --release
./target/release/aztui
```

## Just Recipes

From repo root:

```bash
just help
just app-run
just app-run-debug
just build
just test
```

## Documentation Map

App documentation lives in `src/apps/aztui/docs/`:

- `BUILDING.md`
- `ARCHITECTURE.md`
- `AZURE_AUTHENTICATION.md`
- `KEYBOARD_SHORTCUTS.md`
- `TUI_ARCHITECTURE.md`
