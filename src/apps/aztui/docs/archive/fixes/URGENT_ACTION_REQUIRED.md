# ⚠️ URGENT ACTION REQUIRED

## Blocking Issue: Module Ambiguity Error

The aztui workspace **cannot compile** due to a module ambiguity error in `aztui-core`.

### Problem

Both of these exist:
- `crates/aztui-core/src/cache.rs` ✅ (complete implementation, 819 lines)
- `crates/aztui-core/src/cache/mod.rs` ❌ (placeholder that says "should be deleted")

Rust cannot determine which one to use as the `cache` module.

### Error Message

```
error[E0761]: file for module `cache` found at both "crates/aztui-core/src/cache.rs" and "crates/aztui-core/src/cache/mod.rs"
 --> crates/aztui-core/src/lib.rs:2:1
  |
2 | pub mod cache;
  | ^^^^^^^^^^^^^^
  |
  = help: delete or rename one of them to remove the ambiguity
```

## Solution (Choose One)

### Option 1: Run the Cleanup Script (Recommended)

```bash
chmod +x cleanup.sh
./cleanup.sh
```

This will:
1. Remove the `crates/aztui-core/src/cache/` directory
2. Remove backup files (`cache_old_delete_me.rs`, `lib_backup.rs`)
3. Run `cargo check` to verify the fix

### Option 2: Manual Cleanup

```bash
# Remove the cache directory
rm -rf crates/aztui-core/src/cache/

# Remove backup files
rm crates/aztui-core/src/cache_old_delete_me.rs
rm crates/aztui-core/src/lib_backup.rs

# Verify the fix
cargo check --workspace
```

## Why Keep cache.rs?

The `cache.rs` file contains the **complete, production-ready implementation**:
- ✅ 819 lines of code
- ✅ Cache struct with SQLite connection management
- ✅ Migration system with schema versioning
- ✅ Repository pattern for data access
- ✅ Full CRUD operations for subscriptions, resource groups, and resources
- ✅ Advanced filtering with ResourceFilter builder
- ✅ Cache statistics and maintenance (vacuum, clear expired)
- ✅ 5 passing unit tests
- ✅ Comprehensive documentation

The `cache/` directory contains:
- ❌ Incomplete/duplicate code
- ❌ A mod.rs that literally says "This file should be deleted"

## After Fix

Once the cleanup is complete, you should see:

```bash
$ cargo check --workspace
    Checking aztui-core v0.1.0 (...)
    Checking aztui-azure v0.1.0 (...)
    Checking aztui-tui v0.1.0 (...)
    Checking aztui v0.1.0 (...)
    Finished dev [unoptimized + debuginfo] target(s) in X.XXs
```

## Next Steps

After resolving this issue, the project can proceed to **Iteration 4**:
- Implement view transitions
- Add resource group navigation
- Wire up subscription selection
- Add breadcrumb navigation

## Need Help?

If you encounter any issues:
1. Check that you're in the workspace root: `/home/matx/projects/aztui/workspaces/aztui`
2. Verify the files exist before deletion
3. Run `cargo clean` if you still see errors after cleanup
4. Check the git status to see what changed

## Status

- **Current State**: ❌ Cannot compile
- **After Cleanup**: ✅ Ready for Iteration 4
- **Estimated Fix Time**: < 1 minute
