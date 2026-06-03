#!/usr/bin/env python3
"""
Fix module ambiguity error in aztui-core by removing conflicting cache.rs file.

This script resolves the E0761 error:
  file for module `cache` found at both "cache.rs" and "cache/mod.rs"

The cache/ directory contains the correct implementation, so we remove cache.rs.
"""

import os
import sys
from pathlib import Path

def main():
    # Find the workspace root
    script_dir = Path(__file__).parent
    cache_rs = script_dir / "crates" / "aztui-core" / "src" / "cache.rs"
    cache_dir = script_dir / "crates" / "aztui-core" / "src" / "cache"
    
    print("=" * 70)
    print("aztui Module Ambiguity Fix")
    print("=" * 70)
    print()
    
    # Check if both exist
    if not cache_rs.exists():
        print(f"✓ cache.rs does not exist - no fix needed!")
        return 0
    
    if not cache_dir.exists():
        print(f"✗ ERROR: cache/ directory does not exist!")
        print(f"  Expected at: {cache_dir}")
        return 1
    
    # Check if cache.rs is empty or a placeholder
    cache_rs_size = cache_rs.stat().st_size
    print(f"Found conflicting files:")
    print(f"  - cache.rs ({cache_rs_size} bytes)")
    print(f"  - cache/ directory (correct implementation)")
    print()
    
    # Delete cache.rs
    print(f"Deleting: {cache_rs}")
    try:
        cache_rs.unlink()
        print("✓ Successfully deleted cache.rs")
        print()
        print("Module ambiguity resolved! You can now run:")
        print("  cargo check")
        print("  cargo test")
        print("  cargo clippy")
        return 0
    except Exception as e:
        print(f"✗ ERROR: Failed to delete cache.rs: {e}")
        return 1

if __name__ == "__main__":
    sys.exit(main())
