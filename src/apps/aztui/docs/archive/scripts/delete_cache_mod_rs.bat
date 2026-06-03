@echo off
REM Delete the conflicting cache/mod.rs file to resolve E0761 module ambiguity error

setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
set "WORKSPACE_ROOT=%SCRIPT_DIR%.."
set "CONFLICTING_FILE=%WORKSPACE_ROOT%\crates\aztui-core\src\cache\mod.rs"

echo Workspace root: %WORKSPACE_ROOT%
echo Conflicting file: %CONFLICTING_FILE%

if not exist "%CONFLICTING_FILE%" (
    echo [OK] File does not exist - nothing to delete!
    exit /b 0
)

REM Verify it's the conflict marker file
findstr /C:"conflicts with" "%CONFLICTING_FILE%" >nul
if errorlevel 1 (
    echo ERROR: File exists but doesn't appear to be the conflict marker!
    echo Content preview:
    type "%CONFLICTING_FILE%"
    echo.
    echo Aborting for safety. Please review manually.
    exit /b 1
)

REM Delete the file
echo.
echo Deleting conflicting file: %CONFLICTING_FILE%
del "%CONFLICTING_FILE%"
echo [OK] File deleted successfully!

REM Verify cache.rs exists
set "CACHE_RS=%WORKSPACE_ROOT%\crates\aztui-core\src\cache.rs"
if exist "%CACHE_RS%" (
    echo [OK] Main cache module exists: %CACHE_RS%
) else (
    echo WARNING: Main cache module not found: %CACHE_RS%
    exit /b 1
)

echo.
echo ============================================================
echo SUCCESS! Module ambiguity resolved.
echo ============================================================
echo.
echo Next steps:
echo   1. Run: cargo check
echo   2. Run: cargo test
echo   3. Run: cargo clippy
echo.
echo The workspace should now compile successfully!
