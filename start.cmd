@echo off
setlocal
cd /d "%~dp0"

where node >nul 2>nul
if errorlevel 1 (
  echo Node.js is niet gevonden. Installeer het via https://nodejs.org en probeer opnieuw.
  pause
  exit /b 1
)

if not exist data mkdir data

echo Signage wordt gestart...
start "Signage server" /min cmd /c "node server.js"
timeout /t 2 /nobreak >nul
start "" http://localhost:8100/

echo.
echo De server draait in een eigen venster ^(geminimaliseerd^).
echo Sluit dat venster om te stoppen.
