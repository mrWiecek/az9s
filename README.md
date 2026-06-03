# az9s

![az9s logo](assets/az9s-logo.jpg)

az9s is the main product in this repository: a keyboard-first Azure browser for the terminal.

The builder is secondary. It exists to generate and iterate on the app codebase, but you can install and use the app directly.

## Quick Product View

| Component | Purpose | Location |
|---|---|---|
| az9s app | Terminal Azure browser | `src/apps/aztui/` |
| builder | Generation/orchestration tool | `src/aztui_builder/` |
| prompts/specs | Builder inputs | `config/` |
| scratch outputs | Temporary generated work | `workspaces/` |

## Install az9s

From repository root:

```bash
cargo install --path src/apps/aztui/crates/aztui --locked
```

Note: the installed binary is currently named `aztui`.

If you prefer not to install globally:

```bash
cd src/apps/aztui
cargo run
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
# run from source
cd src/apps/aztui
cargo run

# run with debug logs
RUST_LOG=debug cargo run

# build release
cargo build --release
./target/release/aztui
```

## Documentation Map

App documentation lives in `src/apps/aztui/docs/`:

- `BUILDING.md`
- `ARCHITECTURE.md`
- `AZURE_AUTHENTICATION.md`
- `KEYBOARD_SHORTCUTS.md`
- `TUI_ARCHITECTURE.md`

Historical generation artifacts are archived under `src/apps/aztui/docs/archive/`.

## Builder (Secondary)

Only needed when you want to regenerate or evolve the app automatically.

```bash
uv sync
uv run aztui-builder run --spec-file config/aztui_app_spec.md --thread-id aztui-dev
```

Resume interrupted builder runs:

```bash
uv run aztui-builder resume --thread-id aztui-dev --resume-json '{"approved": true}'
```
