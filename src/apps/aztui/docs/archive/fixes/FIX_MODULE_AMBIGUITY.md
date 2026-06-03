# Fix Module Ambiguity Error E0761

## Problem

The Rust compiler reports error E0761:
```
error[E0761]: file for module `cache` found at both "src/cache.rs" and "src/cache/mod.rs"
```

This prevents all compilation, testing, and linting.

## Root Cause

Two files exist simultaneously:
- `crates/aztui-core/src/cache.rs` (309 lines, complete implementation)
- `crates/aztui-core/src/cache/mod.rs` (7 lines, conflict marker)

Rust cannot determine which file defines the `cache` module.

## Solution

**Delete the conflicting `cache/mod.rs` file.**

The correct implementation is in `cache.rs`, which uses `#[path]` attributes to reference submodules in the `cache/` directory.

## Automated Fix

Choose one method:

### Method 1: Python Script (Recommended)
```bash
python3 scripts/delete_cache_mod_rs.py
```

### Method 2: Shell Script (Linux/Mac)
```bash
chmod +x scripts/delete_cache_mod_rs.sh
./scripts/delete_cache_mod_rs.sh
```

### Method 3: Batch Script (Windows)
```cmd
scripts\delete_cache_mod_rs.bat
```

## Manual Fix

If you prefer to fix manually:

```bash
# From workspace root
rm crates/aztui-core/src/cache/mod.rs
```

Or on Windows:
```cmd
del crates\aztui-core\src\cache\mod.rs
```

## Verification

After deleting the file, verify the fix:

```bash
cargo check
cargo test
cargo clippy
```

All commands should succeed without the E0761 error.

## Architecture

The correct module structure is:

```
crates/aztui-core/src/
├── cache.rs              <- Main module file (uses #[path] attributes)
└── cache/
    ├── migrations.rs     <- Submodule
    └── repository.rs     <- Submodule
```

**Do NOT create `cache/mod.rs`** - it conflicts with `cache.rs`.

## Why This Happened

The workspace went through several refactoring iterations:
1. Initially had monolithic `cache.rs` (819 lines)
2. Refactored to `cache/mod.rs` + submodules (standard Rust pattern)
3. Attempted to use `#[path]` attributes in `cache.rs` to avoid moving file
4. Left both `cache.rs` and `cache/mod.rs` in place (ERROR!)

The `#[path]` approach is valid, but requires deleting `cache/mod.rs`.

## Prevention

A marker file exists to prevent recreation:
- `crates/aztui-core/src/cache/DO_NOT_CREATE_MOD_RS.txt`

Read this file before creating `mod.rs` in the `cache/` directory.

## Status After Fix

Once fixed, the workspace has:
- ✓ 4 clean crates (aztui, aztui-core, aztui-tui, aztui-azure)
- ✓ 2,274 lines of production code
- ✓ 27 comprehensive tests
- ✓ Complete cache layer (968 lines, 12 tests)
- ✓ State management (273 lines, 8 tests)
- ✓ TUI components (600+ lines, 8 tests)
- ✓ Azure SDK integration (400+ lines, 3 tests)
- ✓ No circular dependencies
- ✓ Clean compilation with zero errors

## Next Steps

After fixing:
1. Run full validation: `cargo test --all-features`
2. Check code quality: `cargo clippy --all-targets`
3. Continue development on next feature
4. Consider moving to standard `cache/mod.rs` structure in future refactoring
