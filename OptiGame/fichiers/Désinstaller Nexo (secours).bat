@echo off
rem Desinstallation de secours, si "Desinstaller Nexo.exe" est bloque par l'antivirus.
start "" powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0OptiGame.ps1" -Uninstall