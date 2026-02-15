@echo off
setlocal

cd /d "%~dp0"
set "APP_EXE=target\release\image_prompt_generator.exe"
set "APP_NAME=image_prompt_generator.exe"

tasklist /FI "IMAGENAME eq %APP_NAME%" | find /I "%APP_NAME%" >nul
if not errorlevel 1 (
  echo [Run] Existing %APP_NAME% is running. Stopping it...
  taskkill /IM "%APP_NAME%" /T >nul 2>nul
  if errorlevel 1 (
    echo [Error] Failed to stop %APP_NAME%.
    echo         Close the app manually and run again.
    pause
    exit /b 1
  )
  timeout /t 1 /nobreak >nul
)

echo [Run] Building release binary...
cargo build --release
if errorlevel 1 (
  echo [Error] Failed to build release binary.
  pause
  exit /b 1
)

echo [Run] Launching ImagePromptGenerator...
start "" "%APP_EXE%" %*

if errorlevel 1 (
  echo [Error] Failed to start %APP_EXE%
  pause
  exit /b 1
)

exit /b 0
