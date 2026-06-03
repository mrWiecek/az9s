@echo off
REM Cleanup script to remove duplicate cache files and resolve module ambiguity

echo Cleaning up duplicate cache files...

REM Remove the monolithic cache.rs (replaced by cache/mod.rs)
if exist "crates\aztui-core\src\cache.rs" (
    echo   Removing crates\aztui-core\src\cache.rs
    del "crates\aztui-core\src\cache.rs"
)

REM Remove old backup files
if exist "crates\aztui-core\src\cache_old_delete_me.rs" (
    echo   Removing crates\aztui-core\src\cache_old_delete_me.rs
    del "crates\aztui-core\src\cache_old_delete_me.rs"
)

if exist "crates\aztui-core\src\lib_backup.rs" (
    echo   Removing crates\aztui-core\src\lib_backup.rs
    del "crates\aztui-core\src\lib_backup.rs"
)

echo.
echo Cleanup complete!
echo.
echo The cache module is now properly structured:
echo   - crates\aztui-core\src\cache\mod.rs (main module)
echo   - crates\aztui-core\src\cache\migrations.rs (schema migrations)
echo   - crates\aztui-core\src\cache\repository.rs (data access)
echo.
echo Run 'cargo check' to verify the fix.
pause
