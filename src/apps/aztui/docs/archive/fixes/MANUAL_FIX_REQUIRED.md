# ⚠️ MANUAL FIX REQUIRED - SEE URGENT_ACTION_REQUIRED.md

**This file has been superseded by `URGENT_ACTION_REQUIRED.md`**

Please see **URGENT_ACTION_REQUIRED.md** for detailed instructions on resolving the module ambiguity error.

## Quick Fix

Run the cleanup script:

```bash
chmod +x cleanup.sh
./cleanup.sh
```

Or manually remove the conflicting files:

```bash
rm -rf crates/aztui-core/src/cache/
rm crates/aztui-core/src/cache_old_delete_me.rs
rm crates/aztui-core/src/lib_backup.rs
cargo check --workspace
```

## Why This Happened

During the data layer implementation (Iteration 3), both a single-file cache module (`cache.rs`) and a directory-based cache module (`cache/mod.rs`) were created, causing a Rust module ambiguity error.

The complete implementation is in `cache.rs` (819 lines, 5 tests, full functionality), so the `cache/` directory should be removed.
