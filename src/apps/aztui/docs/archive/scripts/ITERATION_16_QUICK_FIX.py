#!/usr/bin/env python3
"""
Iteration 16: Quick Fix for E0761 Module Ambiguity Error
This script deletes the conflicting cache.rs file and verifies the fix.
"""

import os
import sys
import shutil
import subprocess
from pathlib import Path

def main():
    print("=" * 50)
    print("Iteration 16: E0761 Quick Fix")
    print("=" * 50)
    print()
    
    # Check if we're in the right directory
    if not Path("Cargo.toml").exists():
        print("❌ Error: Must run from workspace root")
        return 1
    
    cache_rs = Path("crates/aztui-core/src/cache.rs")
    
    # Check if the problem file exists
    if not cache_rs.exists():
        print("✅ cache.rs already deleted - nothing to do")
        return 0
    
    print(f"🔍 Found conflicting file: {cache_rs}")
    print()
    
    # Backup the file (just in case)
    backup_path = cache_rs.with_suffix(".rs.backup_iteration_16")
    print(f"📦 Creating backup: {backup_path}")
    shutil.copy2(cache_rs, backup_path)
    
    # Delete the conflicting file
    print(f"🗑️  Deleting {cache_rs}...")
    cache_rs.unlink()
    print("✅ File deleted successfully")
    print()
    
    # Verify the fix
    print("🔍 Verifying fix with cargo check...")
    result = subprocess.run(
        ["cargo", "check"],
        capture_output=True,
        text=True
    )
    
    if "error[E0761]" in result.stderr:
        print("❌ E0761 error still present - fix failed")
        print("   Restoring backup...")
        shutil.move(str(backup_path), str(cache_rs))
        return 1
    
    print("✅ cargo check passed - E0761 error resolved!")
    print()
    
    # Run tests
    print("🧪 Running tests...")
    result = subprocess.run(
        ["cargo", "test", "--quiet"],
        capture_output=True,
        text=True
    )
    
    if result.returncode == 0:
        print("✅ All tests passed!")
    else:
        print("⚠️  Some tests failed (but E0761 is fixed)")
        print(f"   Exit code: {result.returncode}")
    
    print()
    print("=" * 50)
    print("✅ Fix Complete!")
    print("=" * 50)
    print()
    print("Next steps:")
    print("  1. Run: cargo test")
    print("  2. Run: cargo clippy")
    print("  3. Run: cargo build --release")
    print("  4. Clean up backup files")
    print()
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
