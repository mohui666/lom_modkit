@echo off
rem Launch the enhanced Legend of Mortal story editor from source.
rem
rem Differs from run_editor.bat: the enhanced build also needs Pillow and UnityPy
rem so it can pull portraits / backgrounds straight out of the installed game.
rem This wrapper checks the venv first and prints the exact install command
rem instead of failing later with a bare ModuleNotFoundError.
setlocal
cd /d "%~dp0"
chcp 65001 >nul
set PYTHONUTF8=1

if not exist ".venv\Scripts\python.exe" (
  echo [ERROR] Virtual environment not found.
  echo.
  echo Create it and install the extra dependencies:
  echo     python -m venv .venv
  echo     .venv\Scripts\python.exe -m pip install PySide6 Pillow UnityPy
  echo.
  pause
  exit /b 1
)

call ".venv\Scripts\activate.bat"
python main.py
if errorlevel 1 (
  echo.
  echo [ERROR] Editor exited with a non-zero status. See crash.log for details.
  pause
)
endlocal
