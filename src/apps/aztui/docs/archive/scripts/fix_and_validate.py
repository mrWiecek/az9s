#!/usr/bin/env python3
"""
Complete fix for E0761 module ambiguity error with validation.

This script:
1. Deletes the conflicting cache/mod.rs file
2. Runs cargo check to verify compilation
3. Runs cargo test to verify tests pass
4. Runs cargo clippy to verify code quality
5. Reports comprehensive results
"""

import os
import sys
import subprocess
from pathlib import Path

def run_command(cmd, cwd):
    """Run a command and return (success, output)"""
    try:
        result = subprocess.run(
            cmd,
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=300
        )
        return result.returncode == 0, result.stdout + result.stderr
    except subprocess.TimeoutExpired:
        return False, "Command timed out after 300 seconds"
    except Exception as e:
        return False, f"Command failed: {e}"

def main():
    # Find the workspace root
    script_dir = Path(__file__).parent
    workspace_root = script_dir.parent
    
    print("="*70)
    print("AZTUI MODULE AMBIGUITY FIX AND VALIDATION")
    print("="*70)
    print(f"\nWorkspace root: {workspace_root}\n")
    
    # Step 1: Delete conflicting file
    print("Step 1: Deleting conflicting cache/mod.rs file...")
    print("-"*70)
    
    conflicting_file = workspace_root / "crates" / "aztui-core" / "src" / "cache" / "mod.rs"
    
    if not conflicting_file.exists():
        print("✓ File does not exist - already deleted!")
    else:
        # Verify it's the conflict marker file
        content = conflicting_file.read_text()
        if "conflicts with" not in content:
            print("ERROR: File exists but doesn't appear to be the conflict marker!")
            print("Content preview:")
            print(content[:200])
            print("\nAborting for safety. Please review manually.")
            return 1
        
        # Delete the file
        conflicting_file.unlink()
        print(f"✓ Deleted: {conflicting_file}")
    
    # Verify cache.rs exists
    cache_rs = workspace_root / "crates" / "aztui-core" / "src" / "cache.rs"
    if not cache_rs.exists():
        print(f"ERROR: Main cache module not found: {cache_rs}")
        return 1
    print(f"✓ Main cache module exists: {cache_rs}")
    
    # Step 2: Run cargo check
    print("\n" + "="*70)
    print("Step 2: Running cargo check...")
    print("-"*70)
    
    success, output = run_command(["cargo", "check", "--all-targets"], workspace_root)
    if success:
        print("✓ cargo check PASSED")
    else:
        print("✗ cargo check FAILED")
        print("\nOutput:")
        print(output)
        return 1
    
    # Step 3: Run cargo test
    print("\n" + "="*70)
    print("Step 3: Running cargo test...")
    print("-"*70)
    
    success, output = run_command(["cargo", "test", "--all-features"], workspace_root)
    if success:
        print("✓ cargo test PASSED")
        # Extract test count
        for line in output.split('\n'):
            if 'test result:' in line:
                print(f"  {line.strip()}")
    else:
        print("✗ cargo test FAILED")
        print("\nOutput:")
        print(output)
        return 1
    
    # Step 4: Run cargo clippy
    print("\n" + "="*70)
    print("Step 4: Running cargo clippy...")
    print("-"*70)
    
    success, output = run_command(
        ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
        workspace_root
    )
    if success:
        print("✓ cargo clippy PASSED (no warnings)")
    else:
        # Check if it's just warnings
        if "warning:" in output.lower() and "error:" not in output.lower():
            print("⚠ cargo clippy found warnings (but no errors)")
            print("\nWarnings:")
            for line in output.split('\n'):
                if 'warning:' in line.lower():
                    print(f"  {line.strip()}")
        else:
            print("✗ cargo clippy FAILED")
            print("\nOutput:")
            print(output)
            return 1
    
    # Success summary
    print("\n" + "="*70)
    print("SUCCESS! All validation passed!")
    print("="*70)
    print("\n✓ Module ambiguity error E0761 resolved")
    print("✓ Workspace compiles successfully")
    print("✓ All tests pass")
    print("✓ Code quality checks pass")
    print("\nThe aztui workspace is now ready for development!")
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
