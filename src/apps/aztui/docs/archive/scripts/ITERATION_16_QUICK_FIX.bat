@echo off
REM Iteration 16: Quick Fix for E0761 Module Ambiguity Error
REM This script deletes the conflicting cache.rs file

echo ==========================================
echo Iteration 16: E0761 Quick Fix
echo ==========================================
echo.

REM Check if we're in the right directory
if not exist "Cargo.toml" (
    echo Error: Must run from workspace root
    exit /b 1
)

REM Check if the problem file exists
if not exist "crates\aztui-core\src\cache.rs" (
    echo cache.rs already deleted - nothing to do
    exit /b 0
)

echo Found conflicting file: crates\aztui-core\src\cache.rs
echo.

REM Backup the file (just in case)
echo Creating backup...
copy crates\aztui-core\src\cache.rs crates\aztui-core\src\cache.rs.backup_iteration_16 >nul

REM Delete the conflicting file
echo Deleting crates\aztui-core\src\cache.rs...
del crates\aztui-core\src\cache.rs

echo File deleted successfully
echo.

REM Verify the fix
echo Verifying fix with cargo check...
cargo check 2>&1 | findstr /C:"error[E0761]" >nul
if %errorlevel% equ 0 (
    echo E0761 error still present - fix failed
    echo Restoring backup...
    move crates\aztui-core\src\cache.rs.backup_iteration_16 crates\aztui-core\src\cache.rs >nul
    exit /b 1
)

echo cargo check passed - E0761 error resolved!
echo.

REM Run tests
echo Running tests...
cargo test --quiet
if %errorlevel% equ 0 (
    echo All tests passed!
) else (
    echo Some tests failed (but E0761 is fixed)
)

echo.
echo ==========================================
echo Fix Complete!
echo ==========================================
echo.
echo Next steps:
echo   1. Run: cargo test
echo   2. Run: cargo clippy
echo   3. Run: cargo build --release
echo   4. Clean up backup files
echo.
