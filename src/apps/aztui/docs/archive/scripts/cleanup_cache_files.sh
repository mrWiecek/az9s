#!/bin/bash
# Cleanup script to remove duplicate cache files and resolve module ambiguity

set -e

echo "🧹 Cleaning up duplicate cache files..."

# Remove the monolithic cache.rs (replaced by cache/mod.rs)
if [ -f "crates/aztui-core/src/cache.rs" ]; then
    echo "  ✓ Removing crates/aztui-core/src/cache.rs"
    rm "crates/aztui-core/src/cache.rs"
fi

# Remove old backup files
if [ -f "crates/aztui-core/src/cache_old_delete_me.rs" ]; then
    echo "  ✓ Removing crates/aztui-core/src/cache_old_delete_me.rs"
    rm "crates/aztui-core/src/cache_old_delete_me.rs"
fi

if [ -f "crates/aztui-core/src/lib_backup.rs" ]; then
    echo "  ✓ Removing crates/aztui-core/src/lib_backup.rs"
    rm "crates/aztui-core/src/lib_backup.rs"
fi

echo ""
echo "✅ Cleanup complete!"
echo ""
echo "The cache module is now properly structured:"
echo "  - crates/aztui-core/src/cache/mod.rs (main module)"
echo "  - crates/aztui-core/src/cache/migrations.rs (schema migrations)"
echo "  - crates/aztui-core/src/cache/repository.rs (data access)"
echo ""
echo "Run 'cargo check' to verify the fix."
