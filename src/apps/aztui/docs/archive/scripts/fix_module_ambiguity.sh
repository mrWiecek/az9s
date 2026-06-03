#!/bin/bash
# Fix module ambiguity error in aztui-core by removing conflicting cache.rs file

set -e

echo "======================================================================"
echo "aztui Module Ambiguity Fix"
echo "======================================================================"
echo ""

CACHE_RS="crates/aztui-core/src/cache.rs"
CACHE_DIR="crates/aztui-core/src/cache"

# Check if both exist
if [ ! -f "$CACHE_RS" ]; then
    echo "✓ cache.rs does not exist - no fix needed!"
    exit 0
fi

if [ ! -d "$CACHE_DIR" ]; then
    echo "✗ ERROR: cache/ directory does not exist!"
    echo "  Expected at: $CACHE_DIR"
    exit 1
fi

# Show what we found
CACHE_RS_SIZE=$(stat -f%z "$CACHE_RS" 2>/dev/null || stat -c%s "$CACHE_RS" 2>/dev/null || echo "unknown")
echo "Found conflicting files:"
echo "  - cache.rs ($CACHE_RS_SIZE bytes)"
echo "  - cache/ directory (correct implementation)"
echo ""

# Delete cache.rs
echo "Deleting: $CACHE_RS"
rm -f "$CACHE_RS"

echo "✓ Successfully deleted cache.rs"
echo ""
echo "Module ambiguity resolved! You can now run:"
echo "  cargo check"
echo "  cargo test"
echo "  cargo clippy"
