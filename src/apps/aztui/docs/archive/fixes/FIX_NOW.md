# 🚨 FIX REQUIRED - 30 SECONDS

## The Problem
```
error[E0761]: file for module `cache` found at both "cache.rs" and "cache/mod.rs"
```

## The Solution (Pick One)

### Option 1: Python Script ⭐ RECOMMENDED
```bash
python3 fix_module_ambiguity.py
```

### Option 2: Shell Script (Linux/macOS)
```bash
./fix_module_ambiguity.sh
```

### Option 3: Batch Script (Windows)
```cmd
fix_module_ambiguity.bat
```

### Option 4: Manual Command
```bash
rm crates/aztui-core/src/cache.rs
```

## Verify Fix
```bash
cargo check
```

**Expected**: ✅ Compilation succeeds

---

**Why**: Empty file `cache.rs` conflicts with `cache/` directory  
**Risk**: None (file is empty and unused)  
**Time**: < 30 seconds  
**Impact**: Unblocks entire project

See **ITERATION_11_CRITICAL_FIX.md** for details.
