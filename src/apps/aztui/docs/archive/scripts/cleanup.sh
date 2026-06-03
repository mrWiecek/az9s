#!/bin/bash
# Cleanup script to resolve module ambiguity in aztui-core

set -e

echo "🧹 Cleaning up aztui-core module conflicts..."

# Remove the cache directory (cache.rs has the complete implementation)
if [ -d "crates/aztui-core/src/cache" ]; then
    echo "  Removing crates/aztui-core/src/cache/ directory..."
    rm -rf crates/aztui-core/src/cache/
fi

# Remove backup files
if [ -f "crates/aztui-core/src/cache_old_delete_me.rs" ]; then
    echo "  Removing crates/aztui-core/src/cache_old_delete_me.rs..."
    rm crates/aztui-core/src/cache_old_delete_me.rs
fi

if [ -f "crates/aztui-core/src/lib_backup.rs" ]; then
    echo "  Removing crates/aztui-core/src/lib_backup.rs..."
    rm crates/aztui-core/src/lib_backup.rs
fi

echo "✅ Cleanup complete!"
echo ""
echo "Running cargo check to verify..."
cargo check --workspace

echo ""
echo "✅ All checks passed! The workspace is now ready for development."
