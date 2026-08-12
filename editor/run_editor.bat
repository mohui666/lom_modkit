@echo off
rem 启动活侠传 Mod 剧情编辑器（激活 venv 后运行 main.py）
cd /d "%~dp0"
call ".venv\Scripts\activate.bat"
python main.py
