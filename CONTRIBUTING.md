# Contributing to c9s

Thanks for helping. c9s is in early development, so the [roadmap](ROADMAP.md) is the
best place to find work that will be accepted.

## Setup

1. Install [Rust](https://rustup.rs) (stable), [just](https://github.com/casey/just) and
   the [Azure CLI](https://learn.microsoft.com/cli/azure/install-azure-cli).
2. Run `az login`.
3. Build and run:
   ```bash
   just run          # release build, logging off
   just run-debug    # debug build, logs to c9s.log
   ```

## Before opening a PR

```bash
just ci   # fmt --check, clippy -D warnings, tests
```

- Keep PRs small and focused on one roadmap item or bug.
- Add tests for logic in `c9s-core` and `c9s-azure` (parsing, filtering, state
  transitions). UI code is tested by rendering into ratatui's `TestBackend`
  where practical.
- **Speed is the feature.** Never `await` network I/O on the UI path.
- **Actions must be safe.** Anything that changes Azure must go through a confirmation
  step and respect `--readonly` (from M3 onward).
- Record user-visible changes under `[Unreleased]` in [CHANGELOG.md](CHANGELOG.md).
- **Bump the version** in any PR that changes the app: `just bump patch` for a fix,
  `minor` for a feature, `major` for a breaking change. Merging it publishes a
  release. CI checks this. See [RELEASING.md](RELEASING.md).
- If you change how components interact, update [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Commit messages

Use short, imperative subjects, optionally with a conventional prefix:
`feat: add VM start action`, `fix: …`, `docs: …`, `refactor: …`.

## License

By contributing, you agree that your contributions are licensed under the
[Apache License 2.0](LICENSE).
