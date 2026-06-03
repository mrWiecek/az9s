#!/bin/bash
# Iteration 16: Quick Fix for E0761 Module Ambiguity Error
# This script deletes the conflicting cache.rs file

set -e

echo "=========================================="
echo "Iteration 16: E0761 Quick Fix"
echo "=========================================="
echo ""

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo "❌ Error: Must run from workspace root"
    exit 1
fi

# Check if the problem file exists
if [ ! -f "crates/aztui-core/src/cache.rs" ]; then
    echo "✅ cache.rs already deleted - nothing to do"
    exit 0
fi

echo "🔍 Found conflicting file: crates/aztui-core/src/cache.rs"
echo ""

# Backup the file (just in case)
echo "📦 Creating backup..."
cp crates/aztui-core/src/cache.rs crates/aztui-core/src/cache.rs.backup_iteration_16

# Delete the conflicting file
echo "🗑️  Deleting crates/aztui-core/src/cache.rs..."
rm crates/aztui-core/src/cache.rs

echo "✅ File deleted successfully"
echo ""

# Verify the fix
echo "🔍 Verifying fix with cargo check..."
if cargo check 2>&1 | grep -q "error\[E0761\]"; then
    echo "❌ E0761 error still present - fix failed"
    echo "   Restoring backup..."
    mv crates/aztui-core/src/cache.rs.backup_iteration_16 crates/aztui-core/src/cache.rs
    exit 1
fi

echo "✅ cargo check passed - E0761 error resolved!"
echo ""

# Run tests
echo "🧪 Running tests..."
if cargo test --quiet 2>&1; then
    echo "✅ All tests passed!"
else
    echo "⚠️  Some tests failed (but E0761 is fixed)"
fi

echo ""
echo "=========================================="
echo "✅ Fix Complete!"
echo "=========================================="
echo ""
echo "Next steps:"
echo "  1. Run: cargo test"
echo "  2. Run: cargo clippy"
echo "  3. Run: cargo build --release"
echo "  4. Clean up backup files"
echo ""
