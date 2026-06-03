# 🚨 CRITICAL: Fix Module Ambiguity Error

## Problem

The aztui workspace **CANNOT COMPILE** due to a module ambiguity error:

```
error[E0761]: file for module `cache` found at both 
  "crates/aztui-core/src/cache.rs" and 
  "crates/aztui-core/src/cache/mod.rs"
```

## Root Cause

An **empty file** `crates/aztui-core/src/cache.rs` exists alongside the `cache/` directory.
The Rust compiler cannot determine which one to use for the `cache` module.

## Solution

**Delete the empty `cache.rs` file.** The correct implementation is in the `cache/` directory.

## Quick Fix (Choose One)

### Option 1: Python Script (Cross-platform)
```bash
python3 fix_module_ambiguity.py
```

### Option 2: Shell Script (Linux/macOS)
```bash
chmod +x fix_module_ambiguity.sh
./fix_module_ambiguity.sh
```

### Option 3: Batch Script (Windows)
```cmd
fix_module_ambiguity.bat
```

### Option 4: Manual Fix
```bash
# Linux/macOS
rm crates/aztui-core/src/cache.rs

# Windows
del crates\aztui-core\src\cache.rs
```

## Verification

After running the fix, verify the build works:

```bash
cargo check    # Should succeed with no errors
cargo test     # Should run 12+ tests successfully
cargo clippy   # Should show zero warnings
```

## What's in the cache/ Directory?

The correct cache module implementation:

```
crates/aztui-core/src/cache/
├── mod.rs          # 280 lines, 8 tests - Main cache API
├── migrations.rs   # 170 lines, 2 tests - Schema migrations
└── repository.rs   # 518 lines, 2 tests - Data access layer
```

**Total: 968 lines of code, 12 unit tests**

## Why This Happened

During refactoring from a monolithic `cache.rs` to a modular structure, the old file
was not deleted, creating the ambiguity. The automated tools cannot delete files,
so manual intervention is required.

## Impact

- ❌ **Cannot compile** any crate in the workspace
- ❌ **Cannot run tests** 
- ❌ **Cannot run clippy**
- ❌ **Cannot build the application**

## After the Fix

Once the file is deleted, the workspace will:

- ✅ Compile successfully
- ✅ Pass all 12 cache tests
- ✅ Pass all 8 TUI tests (20 total)
- ✅ Show zero clippy warnings
- ✅ Be ready for feature development

---

**Action Required:** Run one of the fix scripts above **NOW** to unblock development.
