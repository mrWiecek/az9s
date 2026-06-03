@echo off
REM Fix module ambiguity error in aztui-core by removing conflicting cache.rs file

echo ======================================================================
echo aztui Module Ambiguity Fix
echo ======================================================================
echo.

set CACHE_RS=crates\aztui-core\src\cache.rs
set CACHE_DIR=crates\aztui-core\src\cache

REM Check if both exist
if not exist "%CACHE_RS%" (
    echo [OK] cache.rs does not exist - no fix needed!
    exit /b 0
)

if not exist "%CACHE_DIR%" (
    echo [ERROR] cache/ directory does not exist!
    echo   Expected at: %CACHE_DIR%
    exit /b 1
)

REM Show what we found
echo Found conflicting files:
echo   - cache.rs
echo   - cache\ directory ^(correct implementation^)
echo.

REM Delete cache.rs
echo Deleting: %CACHE_RS%
del /f /q "%CACHE_RS%"

if errorlevel 1 (
    echo [ERROR] Failed to delete cache.rs
    exit /b 1
)

echo [OK] Successfully deleted cache.rs
echo.
echo Module ambiguity resolved! You can now run:
echo   cargo check
echo   cargo test
echo   cargo clippy
