#!/usr/bin/env python3
"""
Delete the conflicting cache/mod.rs file to resolve E0761 module ambiguity error.

This script safely removes the file that causes the Rust compiler to fail.
After running this script, the workspace should compile successfully.
"""

import os
import sys
from pathlib import Path

def main():
    # Find the workspace root
    script_dir = Path(__file__).parent
    workspace_root = script_dir.parent
    
    # Path to the conflicting file
    conflicting_file = workspace_root / "crates" / "aztui-core" / "src" / "cache" / "mod.rs"
    
    print(f"Workspace root: {workspace_root}")
    print(f"Conflicting file: {conflicting_file}")
    
    if not conflicting_file.exists():
        print("✓ File does not exist - nothing to delete!")
        return 0
    
    # Verify it's the conflict marker file
    content = conflicting_file.read_text()
    if "conflicts with ../cache.rs" not in content and "conflicts with cache.rs" not in content:
        print("ERROR: File exists but doesn't appear to be the conflict marker!")
        print("Content preview:")
        print(content[:200])
        print("\nAborting for safety. Please review manually.")
        return 1
    
    # Delete the file
    print(f"\nDeleting conflicting file: {conflicting_file}")
    conflicting_file.unlink()
    print("✓ File deleted successfully!")
    
    # Verify cache.rs exists
    cache_rs = workspace_root / "crates" / "aztui-core" / "src" / "cache.rs"
    if cache_rs.exists():
        print(f"✓ Main cache module exists: {cache_rs}")
    else:
        print(f"WARNING: Main cache module not found: {cache_rs}")
        return 1
    
    print("\n" + "="*60)
    print("SUCCESS! Module ambiguity resolved.")
    print("="*60)
    print("\nNext steps:")
    print("  1. Run: cargo check")
    print("  2. Run: cargo test")
    print("  3. Run: cargo clippy")
    print("\nThe workspace should now compile successfully!")
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
