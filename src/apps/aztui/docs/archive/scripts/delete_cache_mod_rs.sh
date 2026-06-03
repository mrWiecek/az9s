#!/bin/bash
# Delete the conflicting cache/mod.rs file to resolve E0761 module ambiguity error

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(dirname "$SCRIPT_DIR")"
CONFLICTING_FILE="$WORKSPACE_ROOT/crates/aztui-core/src/cache/mod.rs"

echo "Workspace root: $WORKSPACE_ROOT"
echo "Conflicting file: $CONFLICTING_FILE"

if [ ! -f "$CONFLICTING_FILE" ]; then
    echo "✓ File does not exist - nothing to delete!"
    exit 0
fi

# Verify it's the conflict marker file
if ! grep -q "conflicts with" "$CONFLICTING_FILE"; then
    echo "ERROR: File exists but doesn't appear to be the conflict marker!"
    echo "Content preview:"
    head -n 10 "$CONFLICTING_FILE"
    echo ""
    echo "Aborting for safety. Please review manually."
    exit 1
fi

# Delete the file
echo ""
echo "Deleting conflicting file: $CONFLICTING_FILE"
rm "$CONFLICTING_FILE"
echo "✓ File deleted successfully!"

# Verify cache.rs exists
CACHE_RS="$WORKSPACE_ROOT/crates/aztui-core/src/cache.rs"
if [ -f "$CACHE_RS" ]; then
    echo "✓ Main cache module exists: $CACHE_RS"
else
    echo "WARNING: Main cache module not found: $CACHE_RS"
    exit 1
fi

echo ""
echo "============================================================"
echo "SUCCESS! Module ambiguity resolved."
echo "============================================================"
echo ""
echo "Next steps:"
echo "  1. Run: cargo check"
echo "  2. Run: cargo test"
echo "  3. Run: cargo clippy"
echo ""
echo "The workspace should now compile successfully!"
