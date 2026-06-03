@echo off
REM Automated fix for cache module ambiguity error (E0761)
REM This script resolves the conflict between cache.rs and cache/mod.rs

echo === Fixing cache module ambiguity ===
echo.

REM Check if the conflicting file exists
if exist "crates\aztui-core\src\cache.rs" (
    echo [OK] Found conflicting file: crates\aztui-core\src\cache.rs
    
    REM Delete the conflicting file
    del /F /Q "crates\aztui-core\src\cache.rs"
    echo [OK] Deleted crates\aztui-core\src\cache.rs
) else (
    echo [ERROR] File crates\aztui-core\src\cache.rs not found
)

echo.
echo === Verifying cache module structure ===

REM Check that cache/mod.rs exists
if exist "crates\aztui-core\src\cache\mod.rs" (
    echo [OK] cache\mod.rs exists
) else (
    echo [ERROR] cache\mod.rs NOT found - this is a problem!
    exit /b 1
)

REM Check that submodules exist
if exist "crates\aztui-core\src\cache\migrations.rs" (
    echo [OK] cache\migrations.rs exists
) else (
    echo [ERROR] cache\migrations.rs NOT found
)

if exist "crates\aztui-core\src\cache\repository.rs" (
    echo [OK] cache\repository.rs exists
) else (
    echo [ERROR] cache\repository.rs NOT found
)

echo.
echo === Running cargo check ===
cargo check --workspace

echo.
echo === SUCCESS ===
echo Cache module ambiguity resolved!
echo You can now run: cargo test --workspace
