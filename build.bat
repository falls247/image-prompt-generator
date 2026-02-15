@echo off
setlocal

title Image Prompt Generator - Build
cd /d "%~dp0"

echo [Build] Checking Rust toolchain...
where cargo >nul 2>nul
if errorlevel 1 (
  echo [Error] Rust toolchain not found.
  echo         Install rustup first: https://rustup.rs/
  echo.
  pause
  exit /b 1
)

echo [Build] Running cargo build --release ...
cargo build --release
if errorlevel 1 (
  echo [Error] Build failed.
  echo.
  pause
  exit /b 1
)

if not exist dist mkdir dist
if not exist dist\ImagePromptGenerator mkdir dist\ImagePromptGenerator
if exist dist\ImagePromptGenerator\app.ico del /F /Q dist\ImagePromptGenerator\app.ico >nul 2>nul

echo [Build] Copying executable...
copy /Y target\release\image_prompt_generator.exe dist\ImagePromptGenerator\ImagePromptGenerator.exe >nul
if errorlevel 1 (
  echo [Error] Failed to copy executable.
  echo.
  pause
  exit /b 1
)

echo [Build] Copying config...
copy /Y config\config.txt dist\ImagePromptGenerator\config.txt >nul
if errorlevel 1 (
  echo [Error] Failed to copy config file.
  echo.
  pause
  exit /b 1
)

echo.
echo [Build] Build complete: dist\ImagePromptGenerator
echo.
pause
exit /b 0
