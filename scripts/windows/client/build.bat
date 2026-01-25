@echo off
@REM This wrapper allows you to double-click the script to run it.

echo Starting Build Process...
echo.

@REM -NoProfile: Starts faster (skips loading user settings)
@REM -ExecutionPolicy Bypass: Ignores the "scripts disabled" security setting
@REM -File: Points to the .ps1 file in the same folder (%~dp0)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0\build.ps1"

@REM If the PowerShell script crashed, pause so you can see the red error message
if %errorlevel% neq 0 (
    echo.
    echo [ERROR] Build Failed! Check the errors above.
    pause
) else (
    echo.
    echo [SUCCESS] Build Complete. Window closing in 3 seconds...
    timeout /t 3 >nul
)
