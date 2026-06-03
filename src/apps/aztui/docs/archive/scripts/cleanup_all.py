#!/usr/bin/env python3
"""
Comprehensive cleanup script for aztui workspace.

Removes all conflicting, backup, and temporary files to ensure clean compilation.
"""

import os
import sys
from pathlib import Path

def main():
    script_dir = Path(__file__).parent
    
    # Files to delete
    files_to_delete = [
        # Critical: Module ambiguity
        "crates/aztui-core/src/cache.rs",
        
        # Backup files
        "crates/aztui-core/src/cache_DELETED_RENAME_ME.rs",
        "crates/aztui-core/src/cache_old_delete_me.rs",
        "crates/aztui-core/src/lib_backup.rs",
    ]
    
    print("=" * 70)
    print("aztui Workspace Cleanup")
    print("=" * 70)
    print()
    
    deleted = []
    not_found = []
    errors = []
    
    for file_path in files_to_delete:
        full_path = script_dir / file_path
        
        if not full_path.exists():
            not_found.append(file_path)
            continue
        
        try:
            size = full_path.stat().st_size
            full_path.unlink()
            deleted.append((file_path, size))
            print(f"✓ Deleted: {file_path} ({size} bytes)")
        except Exception as e:
            errors.append((file_path, str(e)))
            print(f"✗ ERROR: {file_path}: {e}")
    
    print()
    print("=" * 70)
    print("Summary")
    print("=" * 70)
    print(f"Deleted: {len(deleted)} files")
    print(f"Not found: {len(not_found)} files")
    print(f"Errors: {len(errors)} files")
    print()
    
    if deleted:
        print("Deleted files:")
        for path, size in deleted:
            print(f"  - {path} ({size} bytes)")
        print()
    
    if errors:
        print("Errors:")
        for path, error in errors:
            print(f"  - {path}: {error}")
        print()
        return 1
    
    print("✓ Cleanup complete! You can now run:")
    print("  cargo check")
    print("  cargo test")
    print("  cargo clippy")
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
