@echo off
rem Lanceur de secours, si Nevermind.exe est bloque par l'antivirus.
rem (OptiGame.ps1 : ancien nom du script, dans une copie installee avec l'ancien zip de compatibilite)
set "script=%~dp0Nevermind.ps1"
if not exist "%script%" set "script=%~dp0OptiGame.ps1"
start "" powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%script%"