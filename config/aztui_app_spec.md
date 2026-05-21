# aztui application specification

Build a Rust terminal application named `aztui`.

## Product intent

`aztui` is a k9s-like Azure browser for operators who want keyboard-first inspection of subscriptions, resource groups, compute, networking, and logs without leaving the terminal.

## UX principles

- Fast startup and responsive navigation.
- Keyboard-first workflows inspired by k9s.
- Multiple panes with clear focus handling.
- Compact but readable information density.
- Command palette and context-aware help.

## Technical targets

- Rust stable toolchain.
- Ratatui + Crossterm.
- Async-friendly architecture.
- SQLite cache for local state and API snapshots.
- Azure authentication through Azure CLI or device code fallback.

## Initial milestones

1. Create a Rust workspace and application shell.
2. Implement terminal app lifecycle and key navigation scaffolding.
3. Add Azure authentication abstraction and subscription selection.
4. Add a local SQLite cache layer.
5. Render a first resource explorer view.
6. Run cargo validation and iterate.
