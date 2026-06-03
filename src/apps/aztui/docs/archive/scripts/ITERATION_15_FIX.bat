@echo off
REM ITERATION 15: Definitive fix for E0761 module ambiguity error

echo ================================================================================
echo ITERATION 15: E0761 Module Ambiguity Fix
echo ================================================================================
echo.

set CONFLICT_FILE=crates\aztui-core\src\cache\mod.rs

if not exist "%CONFLICT_FILE%" (
    echo SUCCESS: %CONFLICT_FILE% does not exist
    echo   The module ambiguity error should be resolved.
    echo.
    echo Next steps:
    echo   1. Run: cargo check
    echo   2. Run: cargo test
    echo   3. Run: cargo clippy
    exit /b 0
)

echo Found conflicting file: %CONFLICT_FILE%
echo.
echo Current content:
echo --------------------------------------------------------------------------------
type "%CONFLICT_FILE%"
echo --------------------------------------------------------------------------------
echo.

REM Delete the file
del "%CONFLICT_FILE%"

echo DELETED: %CONFLICT_FILE%
echo.
echo SUCCESS: Module ambiguity error should now be resolved!
echo.
echo Next steps:
echo   1. Run: cargo check
echo   2. Run: cargo test
echo   3. Run: cargo clippy
echo.
