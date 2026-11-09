@echo off
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust (cargo) is niet gevonden. Installeer het via https://rustup.rs en probeer opnieuw.
  exit /b 1
)

echo Signage-installer bouwen...
cargo build --release
if errorlevel 1 exit /b 1

if not exist "..\dist" mkdir "..\dist"
copy /y "target\release\Signage-Setup.exe" "..\dist\Signage-Setup.exe" >nul
if errorlevel 1 exit /b 1

echo.
echo Klaar: dist\Signage-Setup.exe
