# Fix Instructions for Cache Module Ambiguity (E0761)

## Quick Fix (30 seconds)

Run this command from the workspace root:

```bash
python3 fix_cache_conflict.py
```

That's it! The script will:
1. Delete the conflicting `cache/mod.rs` file
2. Verify the module structure
3. Run `cargo check --workspace`
4. Report success

## What's the Problem?

The workspace cannot compile because Rust found two files for the cache module:
- `crates/aztui-core/src/cache.rs` (9,505 bytes, correct version)
- `crates/aztui-core/src/cache/mod.rs` (310 bytes, conflict marker)

Rust error E0761 means: "I found both cache.rs AND cache/mod.rs, which one should I use?"

## Why Does This Happen?

Rust's module system looks for modules in two places:
1. `module_name.rs` (sibling file)
2. `module_name/mod.rs` (directory with mod.rs)

When BOTH exist, Rust refuses to compile.

## The Solution

We keep `cache.rs` because it:
- Has the complete implementation (9,505 bytes)
- Uses `#[path]` attributes to reference submodules
- Includes all 8 unit tests
- Provides clean API

We delete `cache/mod.rs` because it:
- Is just a conflict marker (310 bytes)
- Causes the E0761 error
- Is no longer needed

## Fix Options

### Option 1: Automated Python Script (Recommended)

```bash
python3 fix_cache_conflict.py
```

**Advantages:**
- Fully automated
- Verifies the fix
- Runs cargo check
- Detailed reporting
- Cross-platform

### Option 2: Shell Script (Linux/Mac)

```bash
chmod +x FIX_CACHE_NOW.sh
./FIX_CACHE_NOW.sh
```

### Option 3: Batch Script (Windows)

```cmd
FIX_CACHE_NOW.bat
```

### Option 4: Manual (Any Platform)

```bash
# Delete the conflicting file
rm crates/aztui-core/src/cache/mod.rs

# Verify the fix
cargo check --workspace
```

## Verification

After running any fix option, verify success:

```bash
# Should compile without errors
cargo check --workspace

# Should run 31 tests
cargo test --workspace

# Should pass linting
cargo clippy --workspace

# Should build successfully
cargo build --workspace
```

## What Happens After the Fix?

Once `cache/mod.rs` is deleted, the module structure becomes:

```
crates/aztui-core/src/
├── lib.rs                    (declares: pub mod cache;)
├── cache.rs                  (main module, uses #[path] attributes)
└── cache/
    ├── migrations.rs         (database migrations)
    └── repository.rs         (data access layer)
```

Rust will:
1. See `pub mod cache;` in lib.rs
2. Look for cache.rs (found!)
3. Load cache.rs as the cache module
4. Process #[path] attributes to load submodules
5. Compile successfully ✅

## Technical Details

### cache.rs Structure

```rust
// Main module file
#[path = "cache/migrations.rs"]
mod migrations;

#[path = "cache/repository.rs"]
mod repository;

// Public API
pub struct Cache { ... }
pub struct CacheStats { ... }
pub use repository::{CacheRepository, ResourceFilter};

// Implementation
impl Cache {
    pub fn new(path: &Path) -> Result<Self> { ... }
    pub fn repository(&self) -> CacheRepository { ... }
    pub fn clear_all(&self) -> Result<()> { ... }
    pub fn clear_expired(&self, ttl_seconds: i64) -> Result<usize> { ... }
    pub fn stats(&self) -> Result<CacheStats> { ... }
    pub fn vacuum(&self) -> Result<()> { ... }
}

// 8 unit tests
#[cfg(test)]
mod tests { ... }
```

### Why #[path] Attributes?

The `#[path]` attribute tells Rust exactly where to find a submodule:

```rust
#[path = "cache/migrations.rs"]
mod migrations;
```

This means:
- "Load the migrations module from cache/migrations.rs"
- "Don't look for migrations.rs in the current directory"
- "Don't look for migrations/mod.rs"

This gives us flexibility to organize code in subdirectories while keeping the main module in a single file.

## Troubleshooting

### "python3: command not found"

Try `python` instead of `python3`:
```bash
python fix_cache_conflict.py
```

Or use the shell/batch scripts instead.

### "Permission denied" on Shell Script

Make it executable:
```bash
chmod +x FIX_CACHE_NOW.sh
./FIX_CACHE_NOW.sh
```

### "cargo: command not found"

Install Rust:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Still Getting E0761 After Fix

Verify the file is actually deleted:
```bash
ls -la crates/aztui-core/src/cache/mod.rs
```

If it still exists, delete it manually:
```bash
rm -f crates/aztui-core/src/cache/mod.rs
```

### Other Compilation Errors

If you get different errors after fixing E0761, that's progress! The module ambiguity is resolved, and you can now address other issues.

## Need Help?

1. **Read the status report**: `ITERATION_12_STATUS.md`
2. **Check the complete guide**: `ITERATION_12_COMPLETE.md`
3. **Review the quickstart**: `QUICKSTART.md`
4. **Check project status**: `PROJECT_STATUS.md`

## Summary

- **Problem**: E0761 module ambiguity error
- **Cause**: Both cache.rs and cache/mod.rs exist
- **Solution**: Delete cache/mod.rs
- **Fix time**: < 1 minute
- **Command**: `python3 fix_cache_conflict.py`

---

**Ready to fix?** Run: `python3 fix_cache_conflict.py`
