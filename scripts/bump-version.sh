#!/usr/bin/env bash
# Bump the workspace version and cut a CHANGELOG section for it.
#
#   scripts/bump-version.sh patch   # bug fix:            0.1.0 -> 0.1.1
#   scripts/bump-version.sh minor   # new feature:        0.1.0 -> 0.2.0
#   scripts/bump-version.sh major   # breaking change:    0.1.0 -> 1.0.0
#
# Moves everything under "## [Unreleased]" in CHANGELOG.md into a new
# "## [X.Y.Z] - YYYY-MM-DD" section, updates the compare links at the bottom,
# and updates Cargo.toml/Cargo.lock. Commit the result in your PR; merging it
# to main publishes the release (see RELEASING.md).
set -euo pipefail
cd "$(dirname "$0")/.."

part="${1:-}"
case "$part" in patch | minor | major) ;; *)
  echo "usage: $0 patch|minor|major" >&2
  exit 2
  ;;
esac

current=$(sed -n 's/^version = "\([0-9.]*\)"$/\1/p' Cargo.toml | head -1)
[[ "$current" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "could not read workspace version from Cargo.toml" >&2
  exit 1
}
IFS=. read -r major minor patch <<<"$current"
case "$part" in
patch) patch=$((patch + 1)) ;;
minor) minor=$((minor + 1)) patch=0 ;;
major) major=$((major + 1)) minor=0 patch=0 ;;
esac
next="$major.$minor.$patch"
today=$(date -u +%Y-%m-%d)

# There must be something to release.
unreleased=$(awk '/^## \[Unreleased\]/{f=1;next} f&&/^## \[/{exit} f' CHANGELOG.md | grep -c '[^[:space:]]' || true)
if [ "$unreleased" -eq 0 ]; then
  echo "CHANGELOG.md has nothing under ## [Unreleased]; describe the change first" >&2
  exit 1
fi

repo_url="https://github.com/mrWiecek/az9s"
VERSION_CURRENT="$current" VERSION_NEXT="$next" TODAY="$today" REPO="$repo_url" perl -0pi -e '
  my ($cur, $next, $today, $repo) = @ENV{qw(VERSION_CURRENT VERSION_NEXT TODAY REPO)};
  s/^## \[Unreleased\]\n/## [Unreleased]\n\n## [$next] - $today\n/m or die "no Unreleased heading\n";
  s/^\[Unreleased\]: .*\n/[Unreleased]: $repo\/compare\/v$next...HEAD\n[$next]: $repo\/compare\/v$cur...v$next\n/m
    or $_ .= "\n[Unreleased]: $repo/compare/v$next...HEAD\n[$next]: $repo/compare/v$cur...v$next\n";
' CHANGELOG.md

VERSION_CURRENT="$current" VERSION_NEXT="$next" perl -pi -e '
  s/^version = "\Q$ENV{VERSION_CURRENT}\E"$/version = "$ENV{VERSION_NEXT}"/ if !$done && /^version = /;
  $done = 1 if /^version = /;
' Cargo.toml

# Refresh the workspace crates' versions in Cargo.lock (no network needed).
cargo metadata --format-version 1 --offline >/dev/null 2>&1 ||
  cargo metadata --format-version 1 >/dev/null 2>&1 || {
  echo "warning: could not refresh Cargo.lock; run 'cargo check' before committing" >&2
}

echo "Bumped $current -> $next ($part). Review CHANGELOG.md, then commit:"
echo "  git commit -am \"release: v$next\""
