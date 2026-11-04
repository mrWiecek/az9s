# Releasing c9s

## Versioning rule

Plain [Semantic Versioning](https://semver.org). No alpha, beta or pre-release tags.

| The change… | Bump | Example |
|---|---|---|
| fixes a bug | **patch** | `0.1.0` → `0.1.1` |
| adds a feature | **minor** | `0.1.1` → `0.2.0` |
| breaks something users rely on (command, key, flag, config, cache format) | **major** | `0.2.0` → `1.0.0` |

Changes that don't touch the app (docs, CI) need no bump.

## Releasing

1. In your PR, describe the change under `## [Unreleased]` in `CHANGELOG.md`.
2. Bump the version:
   ```bash
   just bump patch    # or: minor / major
   ```
   This updates `Cargo.toml` and `Cargo.lock`, moves the `[Unreleased]` entries
   into a new `## [X.Y.Z] - date` section, and updates the compare links.
3. Commit and merge the PR.

On merge to `main`, the [Release workflow](.github/workflows/release.yml) sees a
version with no tag and does the rest:
- builds binaries for Linux x86_64, macOS (Apple Silicon and Intel) and Windows
  x86_64
- tags the merged commit `vX.Y.Z`
- publishes a [GitHub Release](https://github.com/mrWiecek/c9s/releases) with the
  binaries, `SHA256SUMS`, and that version's CHANGELOG section as release notes

CI fails a PR that changes code under `crates/` or `Cargo.*` without bumping an
already-released version, or without a CHANGELOG section for the new version.

## Audit trail

For any version you can answer *what shipped, when, from which code*:

| Question | Where |
|---|---|
| What changed in `vX.Y.Z`? | `CHANGELOG.md` section, also on the GitHub Release page |
| Which code is it? | Tag `vX.Y.Z` → commit → the PR that merged it |
| What changed between two versions? | `https://github.com/mrWiecek/c9s/compare/vA...vB`, also linked at the bottom of `CHANGELOG.md` |
| Which build is a user running? | `c9s --version` prints the version and commit, e.g. `c9s 0.1.1 (3f2c1a9b0d4e)` |
| Is a downloaded binary genuine? | Compare its hash with `SHA256SUMS` on the release |

**Tags are never moved or deleted.** A bad release is fixed by a newer release,
so the history stays complete.

## Rollback

### Users: go back to an earlier version

Download the archive for your platform from that version's
[release page](https://github.com/mrWiecek/c9s/releases), or build it from its tag:

```bash
cargo install --git https://github.com/mrWiecek/c9s --tag v0.1.0 --locked c9s
```

Check with `c9s --version`. The local cache is only a cache: if an older
version can't read it, delete it and it is rebuilt from Azure.

| OS | Cache location |
|---|---|
| Linux / WSL | `~/.local/share/c9s/` |
| macOS | `~/Library/Application Support/c9s/` |
| Windows | `%LOCALAPPDATA%\c9s\data\` |

### Maintainers: a release is broken

1. Revert the offending change on a branch: `git revert <merge-commit>`.
2. Add a `### Fixed` entry such as "Reverted X from 0.4.0 because Y", then run
   `just bump patch`.
3. Merge it. The next patch release ships the fix, and `vX.Y.Z` stays as a
   record of what went wrong.
4. Optionally add a warning to the bad release's notes on GitHub, pointing to
   the fixed version.
