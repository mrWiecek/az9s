#!/usr/bin/env bash
# CI guard for pull requests: code changes must come with a version bump.
#
#   scripts/check-version.sh <base-ref>     e.g. origin/main
#
# - If nothing under crates/ or Cargo.* changed, no bump is needed (docs, CI).
# - If the base version was never released (no vX.Y.Z tag), the pending
#   release absorbs this change; no bump needed.
# - Otherwise the version must be higher than the base version, and
#   CHANGELOG.md must have a "## [X.Y.Z]" section for it.
set -euo pipefail
base="${1:?usage: $0 <base-ref>}"

version_of() { sed -n 's/^version = "\([0-9.]*\)"$/\1/p' | head -1; }
current=$(version_of <Cargo.toml)
previous=$(git show "$base:Cargo.toml" | version_of)

code_changes=$(git diff --name-only "$base"...HEAD | grep -E '^(crates/|Cargo\.toml$|Cargo\.lock$)' || true)
if [ -z "$code_changes" ]; then
  echo "No code changes: no version bump needed (version $current)."
  exit 0
fi

if [ "$current" = "$previous" ]; then
  if git rev-parse -q --verify "refs/tags/v$previous" >/dev/null; then
    echo "::error::Code changed but the version is still $current, which is already released."
    echo "Run 'just bump patch' (fix), 'just bump minor' (feature) or 'just bump major' (breaking) and commit."
    exit 1
  fi
  echo "v$current is not released yet; this change ships with it."
else
  highest=$(printf '%s\n%s\n' "$previous" "$current" | sort -V | tail -1)
  if [ "$highest" != "$current" ]; then
    echo "::error::Version went backwards: $previous -> $current"
    exit 1
  fi
  echo "Version bumped: $previous -> $current"
fi

if ! grep -q "^## \[$current\]" CHANGELOG.md; then
  echo "::error::CHANGELOG.md has no '## [$current]' section. 'just bump <part>' creates it."
  exit 1
fi
echo "CHANGELOG.md has a section for $current."
