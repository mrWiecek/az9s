#!/bin/bash
# Automated fix for cache module ambiguity error (E0761)
# This script resolves the conflict between cache.rs and cache/mod.rs

set -e

echo "=== Fixing cache module ambiguity ==="
echo ""

# Check if the conflicting file exists
if [ -f "crates/aztui-core/src/cache.rs" ]; then
    echo "✓ Found conflicting file: crates/aztui-core/src/cache.rs"
    
    # Delete the conflicting file
    rm -f "crates/aztui-core/src/cache.rs"
    echo "✓ Deleted crates/aztui-core/src/cache.rs"
else
    echo "✗ File crates/aztui-core/src/cache.rs not found"
fi

echo ""
echo "=== Verifying cache module structure ==="

# Check that cache/mod.rs exists
if [ -f "crates/aztui-core/src/cache/mod.rs" ]; then
    echo "✓ cache/mod.rs exists"
else
    echo "✗ cache/mod.rs NOT found - this is a problem!"
    exit 1
fi

# Check that submodules exist
if [ -f "crates/aztui-core/src/cache/migrations.rs" ]; then
    echo "✓ cache/migrations.rs exists"
else
    echo "✗ cache/migrations.rs NOT found"
fi

if [ -f "crates/aztui-core/src/cache/repository.rs" ]; then
    echo "✓ cache/repository.rs exists"
else
    echo "✗ cache/repository.rs NOT found"
fi

echo ""
echo "=== Running cargo check ==="
cargo check --workspace

echo ""
echo "=== SUCCESS ==="
echo "Cache module ambiguity resolved!"
echo "You can now run: cargo test --workspace"
