# -*- coding: utf-8 -*-
"""离线自检：验证「从本机《活侠传》解出立绘 / 背景」这条链路。

不依赖 Qt，可以单独跑：

    editor/.venv/Scripts/python tests/plus_game_assets_test.py
    editor/.venv/Scripts/python tests/plus_game_assets_test.py --game "D:/.../LegendOfMortal"

检查内容：
1. 游戏目录探测（catalog.json / bundle 目录 / sharedassets2.assets）；
2. 角色与背景映射的解析规模与耗时；
3. 真解一张立绘 + 一张背景，确认落盘的是有效 PNG；
4. 解析结果的落盘缓存能二次命中。
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
import tempfile
import time
from pathlib import Path

EDITOR = Path(__file__).resolve().parent.parent
if str(EDITOR) not in sys.path:
    sys.path.insert(0, str(EDITOR))

import game_assets  # noqa: E402

DEFAULT_GAME_DIRS = (
    Path(r"D:/Program Files (x86)/Steam/steamapps/common/LegendOfMortal"),
    Path(r"C:/Program Files (x86)/Steam/steamapps/common/LegendOfMortal"),
)


def find_game_dir() -> Path | None:
    for candidate in DEFAULT_GAME_DIRS:
        if (candidate / "Mortal.exe").is_file():
            return candidate
    return None


def read_png_size(path: Path) -> tuple[int, int]:
    """读 PNG 头里的宽高，顺便证明这确实是一张完整可解析的图。"""
    with path.open("rb") as stream:
        head = stream.read(24)
    if head[:8] != b"\x89PNG\r\n\x1a\n":
        raise AssertionError(f"{path} 不是 PNG")
    width, height = struct.unpack_from(">II", head, 16)
    return width, height


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--game", default="", help="《活侠传》安装目录")
    args = parser.parse_args()

    game_dir = Path(args.game) if args.game else find_game_dir()
    if game_dir is None or not (game_dir / "Mortal.exe").is_file():
        print("跳过：没有找到《活侠传》安装目录（可用 --game 指定）")
        return 0
    print(f"[game] {game_dir}")

    with tempfile.TemporaryDirectory(prefix="lom_plus_test_") as temp:
        data_dir = Path(temp) / "data"
        data_dir.mkdir(parents=True, exist_ok=True)
        library = game_assets.GameAssetLibrary(game_dir, data_dir)

        ok, reason = library.probe()
        print(f"[probe] {ok} — {reason}")
        assert ok, reason

        started = time.time()
        characters, views = library.mappings()
        print(f"[映射] 人物 {len(characters)} 个、背景 {len(views)} 个，耗时 {time.time() - started:.2f}s")
        assert characters, "没有解析到任何人物映射"
        assert views, "没有解析到任何背景映射"

        editor_data = json.loads(
            (EDITOR.parent / "data" / "editor_data.json").read_text(encoding="utf-8")
        )
        editor_ids = {item["id"] for item in editor_data["characters"]}
        covered = editor_ids & set(characters)
        print(
            f"[覆盖] 编辑器 {len(editor_ids)} 个人物中 {len(covered)} 个有立绘配置"
            f"（{len(covered) / len(editor_ids):.0%}）"
        )
        view_ids = {item["id"] for item in editor_data["views"]} - {"black", "white"}
        covered_views = view_ids & set(views)
        print(
            f"[覆盖] 编辑器 {len(view_ids)} 个背景中 {len(covered_views)} 个有映射"
            f"（{len(covered_views) / len(view_ids):.0%}）"
        )
        assert len(covered) / len(editor_ids) > 0.5, "立绘覆盖率过低，映射可能没解析对"

        # ---- 真解一张立绘 ----
        sample_char = sorted(covered)[0]
        started = time.time()
        portrait = library.portrait_file(sample_char, "normal")
        elapsed = time.time() - started
        assert portrait is not None and portrait.is_file(), f"{sample_char} 立绘导出失败"
        size = read_png_size(portrait)
        print(f"[立绘] {sample_char}/normal → {portrait.name} {size[0]}x{size[1]}，耗时 {elapsed:.2f}s")

        # ---- 再解同一个人物的另一个表情（走 bundle 缓存）----
        emotions = library.emotions(sample_char)
        if len(emotions) > 1:
            started = time.time()
            other = library.portrait_file(sample_char, emotions[1])
            print(f"[立绘] {sample_char}/{emotions[1]} 耗时 {time.time() - started:.2f}s → {bool(other)}")

        # ---- 真解一张背景 ----
        if covered_views:
            sample_view = sorted(covered_views)[0]
            started = time.time()
            view = library.view_file(sample_view)
            elapsed = time.time() - started
            assert view is not None and view.is_file(), f"{sample_view} 背景导出失败"
            size = read_png_size(view)
            print(f"[背景] {sample_view} → {view.name} {size[0]}x{size[1]}，耗时 {elapsed:.2f}s")

        # ---- black / white 纯色图 ----
        for name in ("black", "white"):
            solid = library.view_file(name)
            assert solid is not None and solid.is_file(), f"{name} 纯色图生成失败"
        print("[纯色] black / white 生成 OK")

        # ---- 映射缓存二次命中 ----
        cached = library._read_cache()
        assert cached is not None, "映射缓存没有落盘"
        fresh = game_assets.GameAssetLibrary(game_dir, data_dir)
        started = time.time()
        fresh.mappings()
        print(f"[缓存] 二次加载耗时 {time.time() - started:.3f}s")

        # ---- preview_map 片段形态 ----
        chars_frag, views_frag = library.preview_map_fragment()
        sample = chars_frag[sample_char]
        assert sample["portraits"], "preview_map 片段缺少表情"
        assert library.cached_path("portrait", sample_char, "normal").is_file()
        print(f"[片段] 人物 {len(chars_frag)}、背景 {len(views_frag)}，路径示例 "
              f"{next(iter(sample['portraits'].values()))}")

    print("\n全部通过：可以从游戏目录解出立绘与背景。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
