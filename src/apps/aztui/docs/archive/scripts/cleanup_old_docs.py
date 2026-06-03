#!/usr/bin/env python3
"""
Clean up old documentation and fix scripts from previous iterations.

This script removes duplicate/obsolete files while keeping the latest versions.
"""

import os
import sys
from pathlib import Path

def main():
    script_dir = Path(__file__).parent
    workspace_root = script_dir.parent
    
    print("="*70)
    print("CLEANUP OLD DOCUMENTATION AND FIX SCRIPTS")
    print("="*70)
    print(f"\nWorkspace root: {workspace_root}\n")
    
    # Files to delete (old/duplicate versions)
    old_files = [
        # Old fix scripts (root level - moved to scripts/)
        "fix_module_ambiguity.py",
        "fix_module_ambiguity.sh",
        "fix_module_ambiguity.bat",
        "fix_cache_conflict.py",
        "fix_cache_module.sh",
        "fix_cache_module.bat",
        "FIX_CACHE_NOW.sh",
        "FIX_CACHE_NOW.bat",
        
        # Old cleanup scripts (root level - moved to scripts/)
        "cleanup.sh",
        "cleanup.bat",
        "cleanup_all.py",
        "cleanup_cache_files.py",
        "cleanup_cache_files.sh",
        "cleanup_cache_files.bat",
        
        # Old documentation (superseded by latest)
        "ACTION_REQUIRED_DELETE_FILES.md",
        "CACHE_MODULE_FIX.md",
        "CRITICAL_BLOCKER_ITERATION_9.md",
        "FEATURE_IMPLEMENTER_ITERATION_10.md",
        "FEATURE_IMPLEMENTER_ITERATION_8.md",
        "FEATURE_IMPLEMENTER_SUMMARY.md",
        "FIX_MODULE_AMBIGUITY_NOW.md",
        "FIX_NOW.md",
        "ITERATION_10_COMPLETE.md",
        "ITERATION_10_STATUS.md",
        "ITERATION_11_CRITICAL_FIX.md",
        "ITERATION_11_SUMMARY.md",
        "ITERATION_11_VALIDATION.md",
        "ITERATION_12_COMPLETE.md",
        "ITERATION_12_FINAL_SUMMARY.md",
        "ITERATION_12_STATUS.md",
        "ITERATION_13_ACTION_REQUIRED.md",
        "ITERATION_13_POST_FIX_VALIDATION.md",
        "ITERATION_13_VALIDATION_REPORT.md",
        "ITERATION_8_SUMMARY.md",
        "ITERATION_9_VALIDATION_SUMMARY.md",
        "ITERATION_PLAN.md",
        "MANUAL_FIX_REQUIRED.md",
        "NEXT_STEPS_AFTER_FIX.md",
        "PROJECT_STATUS.md",
        "QUICKSTART.md",
        "QUICK_FIX.md",
        "README_FIX_INSTRUCTIONS.md",
        "REMEDIATION_STEPS.md",
        "TESTER_VALIDATOR_ITERATION_9.json",
        "URGENT_ACTION_REQUIRED.md",
        "VALIDATION_REPORT.md",
        "VALIDATION_SUMMARY.md",
    ]
    
    # Keep these files (current/essential)
    keep_files = [
        "README.md",
        "CRITICAL_FIX_REQUIRED.md",
        "QUICK_FIX.txt",
        "ARCHITECTURE.md",
        "Cargo.toml",
        "Cargo.lock",
        ".gitignore",
    ]
    
    deleted_count = 0
    not_found_count = 0
    
    print("Deleting old files...")
    print("-"*70)
    
    for filename in old_files:
        filepath = workspace_root / filename
        if filepath.exists():
            filepath.unlink()
            print(f"✓ Deleted: {filename}")
            deleted_count += 1
        else:
            not_found_count += 1
    
    print("\n" + "="*70)
    print("CLEANUP SUMMARY")
    print("="*70)
    print(f"\n✓ Deleted: {deleted_count} files")
    print(f"  Not found: {not_found_count} files (already deleted)")
    
    print("\n" + "="*70)
    print("CURRENT DOCUMENTATION STRUCTURE")
    print("="*70)
    print("""
Root Level (Essential):
  ✓ README.md                    - Main documentation
  ✓ CRITICAL_FIX_REQUIRED.md     - Critical fix instructions
  ✓ QUICK_FIX.txt                - Quick reference card
  ✓ ARCHITECTURE.md              - Architecture documentation

Scripts (scripts/):
  ✓ delete_cache_mod_rs.py       - Python fix script
  ✓ delete_cache_mod_rs.sh       - Shell fix script
  ✓ delete_cache_mod_rs.bat      - Batch fix script
  ✓ fix_and_validate.py          - Fix + validation script
  ✓ cleanup_old_docs.py          - This cleanup script

Documentation (docs/):
  ✓ FIX_MODULE_AMBIGUITY.md      - Detailed fix guide
  ✓ ITERATION_14_SUMMARY.md      - Current iteration summary

Crates (crates/):
  ✓ aztui/                       - Main binary
  ✓ aztui-core/                  - Core logic
  ✓ aztui-azure/                 - Azure SDK integration
  ✓ aztui-tui/                   - TUI components
""")
    
    print("="*70)
    print("NEXT STEPS")
    print("="*70)
    print("""
1. Fix the module ambiguity:
   rm crates/aztui-core/src/cache/mod.rs

2. Verify the fix:
   cargo check
   cargo test
   cargo clippy

3. Continue development!
""")
    
    return 0

if __name__ == "__main__":
    sys.exit(main())
