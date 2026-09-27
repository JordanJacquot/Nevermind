@echo off
rem Lanceur de secours, si OptiGame.exe est bloque par l'antivirus.
start "" powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0OptiGame.ps1"