@echo off
REM CRITICAL FIX: Delete cache.rs to resolve E0761 module ambiguity error
REM This script MUST be run before the workspace can compile

echo ==========================================
echo FIXING E0761 MODULE AMBIGUITY ERROR
echo ==========================================
echo.

set CACHE_RS=crates\aztui-core\src\cache.rs

if exist "%CACHE_RS%" (
    echo [OK] Found conflicting file: %CACHE_RS%
    echo   Deleting...
    del "%CACHE_RS%"
    echo   [OK] Deleted successfully
) else (
    echo [OK] File already deleted: %CACHE_RS%
)

echo.
echo Cleaning up old backup files...
if exist crates\aztui-core\src\cache.rs.DELETED_ITERATION_15 del crates\aztui-core\src\cache.rs.DELETED_ITERATION_15
if exist crates\aztui-core\src\cache_DELETED_RENAME_ME.rs del crates\aztui-core\src\cache_DELETED_RENAME_ME.rs
if exist crates\aztui-core\src\cache_old_delete_me.rs del crates\aztui-core\src\cache_old_delete_me.rs
if exist crates\aztui-core\src\lib_backup.rs del crates\aztui-core\src\lib_backup.rs
if exist crates\aztui-core\src\cache.rs.DELETED del crates\aztui-core\src\cache.rs.DELETED
if exist crates\aztui-core\src\cache_rs_deleted.txt del crates\aztui-core\src\cache_rs_deleted.txt
echo [OK] Cleanup complete

echo.
echo ==========================================
echo VALIDATING FIX
echo ==========================================
echo.

echo Running cargo check...
cargo check
if %ERRORLEVEL% NEQ 0 (
    echo [FAIL] cargo check FAILED
    exit /b 1
)
echo [OK] cargo check PASSED

echo.
echo Running cargo test...
cargo test
if %ERRORLEVEL% NEQ 0 (
    echo [FAIL] cargo test FAILED
    exit /b 1
)
echo [OK] cargo test PASSED

echo.
echo Running cargo clippy...
cargo clippy -- -D warnings
if %ERRORLEVEL% NEQ 0 (
    echo [FAIL] cargo clippy FAILED
    exit /b 1
)
echo [OK] cargo clippy PASSED

echo.
echo ==========================================
echo [OK] FIX SUCCESSFUL - ALL VALIDATION PASSED
echo ==========================================
echo.
echo The workspace is now ready for development!
