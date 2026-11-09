@echo off
setlocal
set "URL=%~1"

if "%URL%"=="" (
  set /p URL=Plak de player-link uit het beheer: 
)

if "%URL%"=="" (
  echo Geen link opgegeven.
  pause
  exit /b 1
)

set "CHROME=%ProgramFiles%\Google\Chrome\Application\chrome.exe"
if not exist "%CHROME%" set "CHROME=%ProgramFiles(x86)%\Google\Chrome\Application\chrome.exe"
if not exist "%CHROME%" set "CHROME=%LocalAppData%\Google\Chrome\Application\chrome.exe"
if not exist "%CHROME%" set "CHROME=%ProgramFiles%\Microsoft\Edge\Application\msedge.exe"

if not exist "%CHROME%" (
  echo Chrome of Edge is niet gevonden.
  pause
  exit /b 1
)

start "" "%CHROME%" --kiosk --no-first-run --disable-pinch --overscroll-history-navigation=0 "%URL%"
