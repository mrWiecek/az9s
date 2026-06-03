#!/bin/bash
# Fix cache module ambiguity error - Iteration 9
# This script deletes conflicting files to resolve E0761

set -e

echo "=========================================="
echo "aztui Cache Module Fix - Iteration 9"
echo "=========================================="
echo ""

# Navigate to workspace root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Working directory: $(pwd)"
echo ""

# Check if conflicting file exists
if [ -f "crates/aztui-core/src/cache.rs" ]; then
    echo "✓ Found conflicting file: crates/aztui-core/src/cache.rs"
    rm "crates/aztui-core/src/cache.rs"
    echo "✓ Deleted: crates/aztui-core/src/cache.rs"
else
    echo "✓ File already deleted: crates/aztui-core/src/cache.rs"
fi

# Clean up backup files
BACKUP_FILES=(
    "crates/aztui-core/src/cache_DELETED_RENAME_ME.rs"
    "crates/aztui-core/src/cache_old_delete_me.rs"
    "crates/aztui-core/src/lib_backup.rs"
)

for file in "${BACKUP_FILES[@]}"; do
    if [ -f "$file" ]; then
        rm "$file"
        echo "✓ Deleted: $file"
    fi
done

echo ""
echo "=========================================="
echo "Running cargo check..."
echo "=========================================="
echo ""

if cargo check; then
    echo ""
    echo "=========================================="
    echo "✅ SUCCESS! Compilation fixed!"
    echo "=========================================="
    echo ""
    echo "Next steps:"
    echo "  1. cargo test          # Run all tests"
    echo "  2. cargo clippy        # Check code quality"
    echo "  3. cargo build         # Build the project"
    echo ""
else
    echo ""
    echo "=========================================="
    echo "❌ FAILED! Compilation still broken"
    echo "=========================================="
    echo ""
    echo "Please check the error messages above."
    exit 1
fi
