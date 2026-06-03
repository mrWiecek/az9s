#!/bin/bash
# CRITICAL FIX: Delete cache.rs to resolve E0761 module ambiguity error
# This script MUST be run before the workspace can compile

set -e

echo "=========================================="
echo "FIXING E0761 MODULE AMBIGUITY ERROR"
echo "=========================================="
echo ""

CACHE_RS="crates/aztui-core/src/cache.rs"

if [ -f "$CACHE_RS" ]; then
    echo "✓ Found conflicting file: $CACHE_RS"
    echo "  Deleting..."
    rm "$CACHE_RS"
    echo "  ✓ Deleted successfully"
else
    echo "✓ File already deleted: $CACHE_RS"
fi

echo ""
echo "Cleaning up old backup files..."
rm -f crates/aztui-core/src/cache.rs.DELETED_ITERATION_15
rm -f crates/aztui-core/src/cache_DELETED_RENAME_ME.rs
rm -f crates/aztui-core/src/cache_old_delete_me.rs
rm -f crates/aztui-core/src/lib_backup.rs
rm -f crates/aztui-core/src/cache.rs.DELETED
rm -f crates/aztui-core/src/cache_rs_deleted.txt
echo "✓ Cleanup complete"

echo ""
echo "=========================================="
echo "VALIDATING FIX"
echo "=========================================="
echo ""

echo "Running cargo check..."
if cargo check 2>&1; then
    echo "✓ cargo check PASSED"
else
    echo "✗ cargo check FAILED"
    exit 1
fi

echo ""
echo "Running cargo test..."
if cargo test 2>&1; then
    echo "✓ cargo test PASSED"
else
    echo "✗ cargo test FAILED"
    exit 1
fi

echo ""
echo "Running cargo clippy..."
if cargo clippy -- -D warnings 2>&1; then
    echo "✓ cargo clippy PASSED"
else
    echo "✗ cargo clippy FAILED"
    exit 1
fi

echo ""
echo "=========================================="
echo "✓ FIX SUCCESSFUL - ALL VALIDATION PASSED"
echo "=========================================="
echo ""
echo "The workspace is now ready for development!"
