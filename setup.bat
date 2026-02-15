@echo off
setlocal

title Image Prompt Generator - Setup
cd /d "%~dp0"

echo [Setup] Checking Rust toolchain...
where cargo >nul 2>nul
if errorlevel 1 (
  echo [Error] Rust toolchain not found.
  echo         Install rustup first: https://rustup.rs/
  echo.
  pause
  exit /b 1
)

echo [Setup] Cargo found:
cargo --version
if errorlevel 1 (
  echo [Error] Failed to run cargo.
  echo.
  pause
  exit /b 1
)

echo.
echo [Setup] Setup complete.
echo.
pause
exit /b 0
