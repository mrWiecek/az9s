@echo off
REM Cleanup script to resolve module ambiguity in aztui-core (Windows)

echo Cleaning up aztui-core module conflicts...

REM Remove the cache directory (cache.rs has the complete implementation)
if exist "crates\aztui-core\src\cache" (
    echo   Removing crates\aztui-core\src\cache\ directory...
    rmdir /s /q "crates\aztui-core\src\cache"
)

REM Remove backup files
if exist "crates\aztui-core\src\cache_old_delete_me.rs" (
    echo   Removing crates\aztui-core\src\cache_old_delete_me.rs...
    del "crates\aztui-core\src\cache_old_delete_me.rs"
)

if exist "crates\aztui-core\src\lib_backup.rs" (
    echo   Removing crates\aztui-core\src\lib_backup.rs...
    del "crates\aztui-core\src\lib_backup.rs"
)

echo Cleanup complete!
echo.
echo Running cargo check to verify...
cargo check --workspace

if %ERRORLEVEL% EQU 0 (
    echo.
    echo All checks passed! The workspace is now ready for development.
) else (
    echo.
    echo Cargo check failed. Please review the errors above.
    exit /b 1
)
