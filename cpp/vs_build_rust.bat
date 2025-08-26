@echo off
setlocal

REM --- Configuration ---
SET "CRATE_NAME=idevice_ffi"
SET "RUST_PROJECT_PATH=%~dp0..\ffi"

echo "--- Rust Build Script Started ---"
echo "Rust Project Path: %RUST_PROJECT_PATH%"
echo "Visual Studio Platform: %1"

REM --- Header File Copy ---
xcopy /Y "%RUST_PROJECT_PATH%\idevice.h" "%~dp0\include\"

REM --- Locate Cargo ---
REM Check if cargo is in the PATH.
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo Error: cargo.exe not found in PATH.
    echo Please ensure the Rust toolchain is installed and configured.
    exit /b 1
)

REM --- Determine Rust Target ---
SET "RUST_TARGET="
IF /I "%~1" == "x64" (
    SET "RUST_TARGET=x86_64-pc-windows-msvc"
)
IF /I "%~1" == "ARM64" (
    SET "RUST_TARGET=aarch64-pc-windows-msvc"
)

IF NOT DEFINED RUST_TARGET (
    echo Error: Unsupported Visual Studio platform '%~1'.
    echo This script supports 'x64' and 'ARM64'.
    exit /b 1
)

echo "Building for Rust target: %RUST_TARGET%"

REM --- Run Cargo Build ---
SET "STATIC_LIB_NAME=%CRATE_NAME%.lib"
SET "BUILT_LIB_PATH=%RUST_PROJECT_PATH%\..\target\%RUST_TARGET%\release\%STATIC_LIB_NAME%"

REM Change to the Rust project directory and run the build.
pushd "%RUST_PROJECT_PATH%"
cargo build --release --target %RUST_TARGET% --features ring,full --no-default-features
if %errorlevel% neq 0 (
    echo Error: Cargo build failed.
    popd
    exit /b 1
)
popd

echo "Cargo build successful."

REM --- Copy Artifacts ---
echo "Copying '%BUILT_LIB_PATH%' to '%2'"
xcopy /Y "%BUILT_LIB_PATH%" "%2"

echo "--- Rust Build Script Finished ---"
exit /b 0
