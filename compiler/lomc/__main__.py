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


def _cmd_detect_watermark(args):
    from .watermark_detector import detect_image

    result = detect_image(args.image)
    if args.json:
        print(result.to_json())
    else:
        print("detected: %s" % ("yes" if result.detected else "no"))
        print("confidence: %.6f" % result.confidence)
        print("protocol_version: %s" % result.protocol_version)
        print("algorithm_version: %s" % result.algorithm_version)
        print("mod_hash: %s" % result.mod_hash)
        print("checksum_status: %s" % result.checksum_status)
        print("ecc_status: %s" % result.ecc_status)
        print("ecc_corrections: %s" % result.ecc_corrections)
        print("scale_factor: %s" % result.scale_factor)
        print("message: %s" % result.message)
    return 0 if result.detected else 2


def _cmd_detect_watermark_video(args):
    from .watermark_video_detector import detect_video

    result = detect_video(
        args.video,
        ffmpeg=args.ffmpeg,
        interval=args.interval,
        max_frames=args.max_frames,
    )
    if args.json:
        print(result.to_json())
    else:
        for key, value in result.to_dict().items():
            print("%s: %s" % (key, value))
    return 0 if result.detected else 2


def main(argv=None):
    parser = argparse.ArgumentParser(
        prog="lomc", description="活侠传 mod 剧情编译器（契约：docs/chs/mod_format.md）"
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

    p_detect = sub.add_parser(
        "detect-watermark", help="离线检测 PNG/JPG 截图中的来源水印"
    )
    p_detect.add_argument("image", help="PNG/JPG 截图路径")
    p_detect.add_argument(
        "--json", action="store_true", help="输出单行 JSON 检测结果"
    )
    p_detect.set_defaults(func=_cmd_detect_watermark)

    p_video = sub.add_parser(
        "detect-watermark-video",
        help="用 FFmpeg 抽帧并离线累积检测视频来源水印",
    )
    p_video.add_argument("video", help="MP4/MKV/MOV/WebM/AVI/M4V 视频路径")
    p_video.add_argument("--ffmpeg", help="FFmpeg 可执行文件路径（默认从 PATH 查找）")
    p_video.add_argument(
        "--interval", type=float, default=2.0, help="抽帧间隔秒数（默认 2）"
    )
    p_video.add_argument(
        "--max-frames", type=int, default=12, help="最多抽取帧数（默认 12）"
    )
    p_video.add_argument("--json", action="store_true", help="输出单行 JSON")
    p_video.set_defaults(func=_cmd_detect_watermark_video)

    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except LomcError as e:
        print("错误: %s" % e, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
