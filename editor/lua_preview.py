# -*- coding: utf-8 -*-
"""右栏：实时 Lua 预览。

通过 sys.path 插入 <项目根>/compiler 后 import lomc（不 pip 安装），
调用 lomc.compile_story(story_dict) 编译当前文档：
- 成功 → 等宽字体显示 Lua 源码
- 失败/lomc 不可用 → 红字显示错误

lomc 期望接口（编译器组实现，见 docs/chs/mod_format.md §4）：
    compile_story(story: dict) -> str   # 返回 Lua 源码；校验/编译失败抛异常
"""
from __future__ import annotations

import sys
import traceback

from PySide6.QtGui import QFontDatabase
from PySide6.QtWidgets import QPlainTextEdit

from glass_theme import DANGER_TEXT
import models

# 编辑器源文件所在目录 → 项目根（冻结态走 models 的 _MEIPASS 推导，见 lom_bundle.spec）
EDITOR_DIR = models.editor_dir()
PROJECT_ROOT = models.project_root()
# 冻结态该目录在包内不存在：import lomc 由 PyInstaller 冻结导入器解析（PYZ）
COMPILER_DIR = PROJECT_ROOT / "compiler"

_lomc = None
_lomc_error: str | None = None


def get_lomc():
    """按契约方式引入 lomc；不可用时返回 (None, 原因)。"""
    global _lomc, _lomc_error
    if _lomc is not None or _lomc_error is not None:
        return _lomc, _lomc_error
    path = str(COMPILER_DIR)
    if path not in sys.path:
        sys.path.insert(0, path)
    try:
        import lomc  # noqa: F401
        if not hasattr(lomc, "compile_story"):
            raise AttributeError("lomc 缺少 compile_story(story) 接口")
        _lomc = lomc
    except Exception as exc:  # lomc 尚未就绪/导入失败时优雅降级
        _lomc_error = f"{type(exc).__name__}: {exc}"
    return _lomc, _lomc_error


def lomc_available() -> bool:
    return get_lomc()[0] is not None


def compile_story(story: dict) -> tuple[str | None, str | None]:
    """编译 story，返回 (lua源码, 错误信息)，二者必居其一。"""
    lomc, err = get_lomc()
    if lomc is None:
        return None, f"lomc 编译器不可用（{err}）。\n预期位置：{COMPILER_DIR}"
    try:
        return lomc.compile_story(story), None
    except Exception:
        return None, traceback.format_exc(limit=8)


class LuaPreview(QPlainTextEdit):
    """只读 Lua 预览框；编译报错时红字显示。"""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setReadOnly(True)
        font = QFontDatabase.systemFont(QFontDatabase.SystemFont.FixedFont)
        font.setPointSize(10)
        self.setFont(font)
        self._ok_style = ""
        # 深色玻璃主题下的可读错误红（与 glass_theme 令牌一致）
        self._err_style = f"QPlainTextEdit {{ color: {DANGER_TEXT}; }}"

    def show_lua(self, lua: str) -> None:
        self.setStyleSheet(self._ok_style)
        self.setPlainText(lua)

    def show_error(self, message: str) -> None:
        self.setStyleSheet(self._err_style)
        self.setPlainText(message.rstrip())
