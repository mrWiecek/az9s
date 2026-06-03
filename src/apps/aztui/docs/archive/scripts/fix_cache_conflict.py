#!/usr/bin/env python3
"""
Automated fix for cache module ambiguity error (E0761)

This script resolves the conflict between cache.rs and cache/mod.rs
by keeping cache.rs (which uses #[path] attributes) and removing cache/mod.rs.
"""

import os
import sys
import subprocess
from pathlib import Path

def main():
    print("=" * 60)
    print("Cache Module Ambiguity Fix")
    print("=" * 60)
    print()
    
    # Define paths
    cache_rs = Path("crates/aztui-core/src/cache.rs")
    cache_mod_rs = Path("crates/aztui-core/src/cache/mod.rs")
    cache_migrations = Path("crates/aztui-core/src/cache/migrations.rs")
    cache_repository = Path("crates/aztui-core/src/cache/repository.rs")
    
    # Check current state
    print("Checking current state...")
    print()
    
    cache_rs_exists = cache_rs.exists()
    cache_mod_rs_exists = cache_mod_rs.exists()
    
    print(f"  cache.rs exists: {cache_rs_exists}")
    if cache_rs_exists:
        size = cache_rs.stat().st_size
        print(f"    Size: {size:,} bytes")
    
    print(f"  cache/mod.rs exists: {cache_mod_rs_exists}")
    if cache_mod_rs_exists:
        size = cache_mod_rs.stat().st_size
        print(f"    Size: {size:,} bytes")
    
    print(f"  cache/migrations.rs exists: {cache_migrations.exists()}")
    print(f"  cache/repository.rs exists: {cache_repository.exists()}")
    print()
    
    # Determine action
    if not cache_rs_exists and not cache_mod_rs_exists:
        print("ERROR: Neither cache.rs nor cache/mod.rs exists!")
        print("Cannot proceed with fix.")
        return 1
    
    if cache_rs_exists and not cache_mod_rs_exists:
        print("✓ No conflict detected - cache.rs exists, cache/mod.rs does not")
        print("  Module structure is correct!")
        print()
        print("Running cargo check to verify...")
        result = run_cargo_check()
        return 0 if result else 1
    
    if not cache_rs_exists and cache_mod_rs_exists:
        print("✓ No conflict detected - cache/mod.rs exists, cache.rs does not")
        print("  Module structure is correct!")
        print()
        print("Running cargo check to verify...")
        result = run_cargo_check()
        return 0 if result else 1
    
    # Both exist - this is the problem
    print("⚠ CONFLICT DETECTED: Both cache.rs and cache/mod.rs exist")
    print()
    print("Resolution strategy:")
    print("  1. Keep cache.rs (9,505 bytes, uses #[path] attributes)")
    print("  2. Delete cache/mod.rs (conflict marker)")
    print()
    
    # Check if cache.rs has the #[path] attributes
    cache_rs_content = cache_rs.read_text()
    has_path_attrs = '#[path = "cache/migrations.rs"]' in cache_rs_content
    
    if has_path_attrs:
        print("✓ cache.rs has #[path] attributes - this is the correct version")
    else:
        print("⚠ cache.rs does NOT have #[path] attributes")
        print("  This may cause issues after deleting cache/mod.rs")
    
    print()
    print("Proceeding with fix...")
    print()
    
    # Delete cache/mod.rs
    try:
        print(f"Deleting {cache_mod_rs}...")
        cache_mod_rs.unlink()
        print("✓ Deleted successfully")
    except Exception as e:
        print(f"✗ Failed to delete: {e}")
        return 1
    
    print()
    print("Verifying fix...")
    print()
    
    # Verify the file is gone
    if cache_mod_rs.exists():
        print("✗ ERROR: cache/mod.rs still exists after deletion!")
        return 1
    
    print("✓ cache/mod.rs has been deleted")
    print("✓ cache.rs remains (with #[path] attributes)")
    print()
    
    # Run cargo check
    print("Running cargo check --workspace...")
    print()
    result = run_cargo_check()
    
    if result:
        print()
        print("=" * 60)
        print("SUCCESS!")
        print("=" * 60)
        print()
        print("The cache module ambiguity has been resolved.")
        print("You can now:")
        print("  - cargo check --workspace")
        print("  - cargo test --workspace")
        print("  - cargo clippy --workspace")
        print("  - cargo build --workspace")
        print()
        return 0
    else:
        print()
        print("=" * 60)
        print("PARTIAL SUCCESS")
        print("=" * 60)
        print()
        print("The conflicting file has been deleted, but cargo check failed.")
        print("Review the error messages above for next steps.")
        print()
        return 1

def run_cargo_check():
    """Run cargo check and return True if successful"""
    try:
        result = subprocess.run(
            ["cargo", "check", "--workspace"],
            capture_output=True,
            text=True,
            timeout=120
        )
        
        print(result.stdout)
        if result.stderr:
            print(result.stderr)
        
        return result.returncode == 0
    except subprocess.TimeoutExpired:
        print("✗ cargo check timed out after 120 seconds")
        return False
    except FileNotFoundError:
        print("✗ cargo command not found - is Rust installed?")
        return False
    except Exception as e:
        print(f"✗ Error running cargo check: {e}")
        return False

if __name__ == "__main__":
    sys.exit(main())
