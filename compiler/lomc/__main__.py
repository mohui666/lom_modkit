# -*- coding: utf-8 -*-
"""lomc 命令行入口。

用法：
    python -m lomc build <story.json> [-o out.lua]   校验并编译为 Lua
    python -m lomc check <story.json>                只校验
    python -m lomc pack  <mod目录> [-o xxx.lommod]   校验 manifest + 全部编译 + 打 zip
"""

import argparse
import os
import sys

from .compiler import compile_story_file, default_lua_path, load_json_file
from .errors import LomcError
from .pack import pack_mod
from .validate import validate_story


def _cmd_build(args):
    lua = compile_story_file(args.story)
    output = args.output or default_lua_path(args.story)
    try:
        with open(output, "w", encoding="utf-8", newline="\n") as f:
            f.write(lua)
    except OSError as e:
        raise LomcError("无法写入输出文件 %s: %s" % (output, e)) from e
    print("编译成功: %s -> %s" % (args.story, output))
    return 0


def _cmd_check(args):
    story = load_json_file(args.story)
    warnings = []
    validate_story(story, source=args.story, warnings=warnings)
    for w in warnings:
        print("警告: %s" % w, file=sys.stderr)
    print("校验通过: %s（%d 个节点）" % (args.story, len(story["nodes"])))
    return 0


def _cmd_pack(args):
    output = pack_mod(args.moddir, output=args.output)
    size = os.path.getsize(output)
    print("打包成功: %s -> %s（%d 字节）" % (args.moddir, output, size))
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(
        prog="lomc", description="活侠传 mod 剧情编译器（契约：docs/mod_format.md）"
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p_build = sub.add_parser("build", help="校验 story.json 并编译为游戏原生 Lua")
    p_build.add_argument("story", help="story.json 路径")
    p_build.add_argument(
        "-o", "--output", help="输出 .lua 路径（默认与输入同名换后缀）"
    )
    p_build.set_defaults(func=_cmd_build)

    p_check = sub.add_parser("check", help="只校验 story.json，不生成文件")
    p_check.add_argument("story", help="story.json 路径")
    p_check.set_defaults(func=_cmd_check)

    p_pack = sub.add_parser("pack", help="校验并打包 mod 目录为 .lommod（zip）")
    p_pack.add_argument("moddir", help="mod 目录（含 manifest.json 和 story/）")
    p_pack.add_argument(
        "-o", "--output", help="输出路径（默认 <mod目录> 同级的 <目录名>.lommod）"
    )
    p_pack.set_defaults(func=_cmd_pack)

    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except LomcError as e:
        print("错误: %s" % e, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
