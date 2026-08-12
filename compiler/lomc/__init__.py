# -*- coding: utf-8 -*-
"""lomc — 活侠传 mod 剧情编译器（story.json -> 游戏原生 Lua）。

格式契约：docs/mod_format.md（v1）。纯 Python 3 标准库，零第三方依赖。
"""

from .compiler import compile_story, compile_story_file, load_json_file
from .errors import LomcError
from .pack import pack_mod
from .validate import validate_manifest, validate_story

__version__ = "1.0.0"

__all__ = [
    "LomcError",
    "compile_story",
    "compile_story_file",
    "load_json_file",
    "pack_mod",
    "validate_manifest",
    "validate_story",
    "__version__",
]
