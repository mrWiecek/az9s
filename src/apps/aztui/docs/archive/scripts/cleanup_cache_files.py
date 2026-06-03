#!/usr/bin/env python3
"""
Cleanup script to remove duplicate cache files and resolve module ambiguity.
This script removes the monolithic cache.rs file and backup files.
"""

import os
import sys
from pathlib import Path

def main():
    print("🧹 Cleaning up duplicate cache files...")
    
    # Files to remove
    files_to_remove = [
        "crates/aztui-core/src/cache.rs",
        "crates/aztui-core/src/cache_old_delete_me.rs",
        "crates/aztui-core/src/lib_backup.rs",
    ]
    
    removed_count = 0
    for file_path in files_to_remove:
        path = Path(file_path)
        if path.exists():
            print(f"  ✓ Removing {file_path}")
            path.unlink()
            removed_count += 1
        else:
            print(f"  ⊘ {file_path} (already removed)")
    
    print()
    if removed_count > 0:
        print(f"✅ Cleanup complete! Removed {removed_count} file(s).")
    else:
        print("✅ All files already cleaned up!")
    
    print()
    print("The cache module is now properly structured:")
    print("  - crates/aztui-core/src/cache/mod.rs (main module)")
    print("  - crates/aztui-core/src/cache/migrations.rs (schema migrations)")
    print("  - crates/aztui-core/src/cache/repository.rs (data access)")
    print()
    print("Run 'cargo check' to verify the fix.")
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
