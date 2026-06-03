#!/usr/bin/env python3
"""
CRITICAL FIX: Delete cache.rs to resolve E0761 module ambiguity error
This script MUST be run before the workspace can compile
Cross-platform Python version
"""

import os
import sys
import subprocess
from pathlib import Path

def main():
    print("=" * 50)
    print("FIXING E0761 MODULE AMBIGUITY ERROR")
    print("=" * 50)
    print()

    # Define files to delete
    cache_rs = Path("crates/aztui-core/src/cache.rs")
    cleanup_files = [
        Path("crates/aztui-core/src/cache.rs.DELETED_ITERATION_15"),
        Path("crates/aztui-core/src/cache_DELETED_RENAME_ME.rs"),
        Path("crates/aztui-core/src/cache_old_delete_me.rs"),
        Path("crates/aztui-core/src/lib_backup.rs"),
        Path("crates/aztui-core/src/cache.rs.DELETED"),
        Path("crates/aztui-core/src/cache_rs_deleted.txt"),
    ]

    # Delete main conflicting file
    if cache_rs.exists():
        print(f"✓ Found conflicting file: {cache_rs}")
        print("  Deleting...")
        cache_rs.unlink()
        print("  ✓ Deleted successfully")
    else:
        print(f"✓ File already deleted: {cache_rs}")

    # Cleanup old backup files
    print()
    print("Cleaning up old backup files...")
    for file in cleanup_files:
        if file.exists():
            file.unlink()
            print(f"  ✓ Deleted: {file}")
    print("✓ Cleanup complete")

    print()
    print("=" * 50)
    print("VALIDATING FIX")
    print("=" * 50)
    print()

    # Run cargo check
    print("Running cargo check...")
    result = subprocess.run(["cargo", "check"], capture_output=True, text=True)
    if result.returncode == 0:
        print("✓ cargo check PASSED")
    else:
        print("✗ cargo check FAILED")
        print(result.stderr)
        sys.exit(1)

    # Run cargo test
    print()
    print("Running cargo test...")
    result = subprocess.run(["cargo", "test"], capture_output=True, text=True)
    if result.returncode == 0:
        print("✓ cargo test PASSED")
    else:
        print("✗ cargo test FAILED")
        print(result.stderr)
        sys.exit(1)

    # Run cargo clippy
    print()
    print("Running cargo clippy...")
    result = subprocess.run(
        ["cargo", "clippy", "--", "-D", "warnings"],
        capture_output=True,
        text=True
    )
    if result.returncode == 0:
        print("✓ cargo clippy PASSED")
    else:
        print("✗ cargo clippy FAILED")
        print(result.stderr)
        sys.exit(1)

    print()
    print("=" * 50)
    print("✓ FIX SUCCESSFUL - ALL VALIDATION PASSED")
    print("=" * 50)
    print()
    print("The workspace is now ready for development!")

if __name__ == "__main__":
    main()
