@echo off
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust/cargo is niet gevonden. Installeer het via https://rustup.rs en probeer opnieuw.
  exit /b 1
)

echo 1/3  Native Signage Studio bouwen...
cargo build --release --manifest-path ..\native\Cargo.toml
if errorlevel 1 exit /b 1

echo 2/3  Verwijderaar bouwen...
cargo build --release --manifest-path uninstaller\Cargo.toml
if errorlevel 1 exit /b 1

echo 3/3  Installer bouwen...
cargo build --release --manifest-path Cargo.toml
if errorlevel 1 exit /b 1

if not exist "..\dist" mkdir "..\dist"
copy /y "target\release\Signage-Studio-Setup.exe" "..\dist\Signage-Studio-Setup.exe" >nul
if errorlevel 1 exit /b 1

echo.
echo Klaar: dist\Signage-Studio-Setup.exe
