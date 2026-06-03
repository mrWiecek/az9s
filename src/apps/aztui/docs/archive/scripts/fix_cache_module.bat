@echo off
REM Fix cache module ambiguity error - Iteration 9
REM This script deletes conflicting files to resolve E0761

echo ==========================================
echo aztui Cache Module Fix - Iteration 9
echo ==========================================
echo.

cd /d "%~dp0"
echo Working directory: %CD%
echo.

REM Check if conflicting file exists
if exist "crates\aztui-core\src\cache.rs" (
    echo + Found conflicting file: crates\aztui-core\src\cache.rs
    del "crates\aztui-core\src\cache.rs"
    echo + Deleted: crates\aztui-core\src\cache.rs
) else (
    echo + File already deleted: crates\aztui-core\src\cache.rs
)

REM Clean up backup files
if exist "crates\aztui-core\src\cache_DELETED_RENAME_ME.rs" (
    del "crates\aztui-core\src\cache_DELETED_RENAME_ME.rs"
    echo + Deleted: crates\aztui-core\src\cache_DELETED_RENAME_ME.rs
)

if exist "crates\aztui-core\src\cache_old_delete_me.rs" (
    del "crates\aztui-core\src\cache_old_delete_me.rs"
    echo + Deleted: crates\aztui-core\src\cache_old_delete_me.rs
)

if exist "crates\aztui-core\src\lib_backup.rs" (
    del "crates\aztui-core\src\lib_backup.rs"
    echo + Deleted: crates\aztui-core\src\lib_backup.rs
)

echo.
echo ==========================================
echo Running cargo check...
echo ==========================================
echo.

cargo check
if %ERRORLEVEL% EQU 0 (
    echo.
    echo ==========================================
    echo SUCCESS! Compilation fixed!
    echo ==========================================
    echo.
    echo Next steps:
    echo   1. cargo test          # Run all tests
    echo   2. cargo clippy        # Check code quality
    echo   3. cargo build         # Build the project
    echo.
) else (
    echo.
    echo ==========================================
    echo FAILED! Compilation still broken
    echo ==========================================
    echo.
    echo Please check the error messages above.
    exit /b 1
)
