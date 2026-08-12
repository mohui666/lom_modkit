# -*- coding: utf-8 -*-
"""编译入口：story dict -> Lua 源码（先校验后 codegen）。"""

import json
import os

from .codegen import story_to_lua
from .errors import LomcError
from .validate import validate_story


def load_json_file(path):
    """读 JSON 文件（容忍 UTF-8 BOM），错误统一转成 LomcError。"""
    try:
        with open(path, "r", encoding="utf-8-sig") as f:
            return json.load(f)
    except FileNotFoundError:
        raise LomcError("文件不存在: %s" % path)
    except json.JSONDecodeError as e:
        raise LomcError("%s: JSON 解析失败（第 %d 行第 %d 列: %s）" % (path, e.lineno, e.colno, e.msg))
    except UnicodeDecodeError as e:
        raise LomcError("%s: 文件不是有效的 UTF-8 编码（%s）" % (path, e))


def compile_story(story, mod_info=None, source=None):
    """校验 + 编译单个 story dict，返回 Lua 源码字符串。"""
    validate_story(story, source or "story.json")
    return story_to_lua(story, mod_info=mod_info, source=source)


def compile_story_file(story_path, mod_info=None):
    """从文件路径校验 + 编译，返回 Lua 源码字符串。"""
    story = load_json_file(story_path)
    # source 用 story/<文件名> 形式，与包内路径一致
    source = "story/%s" % os.path.basename(story_path)
    return compile_story(story, mod_info=mod_info, source=source)


def default_lua_path(story_path):
    """build 命令的默认输出路径：与输入同目录、同名换 .lua 后缀。"""
    base = story_path
    if base.lower().endswith(".json"):
        base = base[: -len(".json")]
    return base + ".lua"
