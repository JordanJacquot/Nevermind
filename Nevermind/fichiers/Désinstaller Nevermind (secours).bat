@echo off
rem Desinstallation de secours, si "Desinstaller Nevermind.exe" est bloque par l'antivirus.
rem (OptiGame.ps1 : ancien nom du script, garde dans le zip pour les anciennes versions de Mindset)
set "script=%~dp0Nevermind.ps1"
if not exist "%script%" set "script=%~dp0OptiGame.ps1"
start "" powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%script%" -Uninstall