#!/usr/bin/env python3
"""Packaged, read-only preview extractor. JSON lines are consumed by the Rust UI.

Uses the existing confirmed StoryCharacterConfig/StoryCharacterData and
AddressableCollectionData layout in _probe_assets, and its exact Sprite lookup.
Does not load game assemblies, start the game, or write into its installation.
"""
import argparse
from functools import lru_cache
import hashlib
import json
from pathlib import Path
import sys


def emit(event, **fields):
    print(json.dumps(dict(event=event, **fields), ensure_ascii=False), flush=True)


def write_json(path, value):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding="utf-8")
    temporary.replace(path)


def token(value):
    # Filenames never come from game strings (including slashes / Windows names).
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def extract(game, output, max_size):
    import UnityPy
    from PIL import Image
    import _probe_assets as probe

    game = game.resolve(strict=True)
    output = output.resolve(strict=True)
    if output == game or game in output.parents:
        raise ValueError("请选择游戏文件夹之外的素材缓存目录")
    if any(output.iterdir()):
        raise ValueError("提取目录必须为空，已有素材不会被覆盖")
    data = game / "Mortal_Data"
    catalog = data / "StreamingAssets/aa/catalog.json"
    bundles = Path(probe.bundle_directory(data))
    probe.SA2 = str(data / "sharedassets2.assets")
    if not catalog.is_file() or not bundles.is_dir() or not Path(probe.SA2).is_file():
        raise ValueError("游戏目录不完整：请选择包含 Mortal_Data 的《活侠传》安装文件夹")
    emit("progress", message="正在读取人物、表情和背景目录", done=0, total=0)
    addresses = probe.parse_catalog(str(catalog))
    characters, views = probe.load_sa2()
    if not characters or not views:
        raise ValueError("没有读到人物或背景目录，当前游戏资源结构可能已改变；请更新提取工具")

    # A single address is exported once; character aliases share its PNG.
    tasks = {}
    mapping = {"characters": {}, "views": {}}
    failures = []
    for cid, character in characters.items():
        portraits = dict(character["portraits"])
        first = next((emotion for emotion, addr in portraits.items() if addr == character["first"]), "first")
        portraits.setdefault(first, character["first"])
        mapping["characters"][cid] = {"name": character["name"], "first": first, "portraits": {}}
        for emotion, address in portraits.items():
            rel = "assets/portraits/" + token(address) + ".png"
            tasks.setdefault(address, (rel, []))[1].append((cid, emotion))
    for key, view in views.items():
        address = view["address"]
        rel = "assets/views/" + token(address) + ".png"
        tasks.setdefault(address, (rel, []))[1].append((None, key))

    @lru_cache(maxsize=2)
    def load_bundle(name):
        path = (bundles / name).resolve(strict=True)
        if bundles.resolve() not in path.parents:
            raise ValueError("素材包路径超出游戏资源目录")
        return UnityPy.load(str(path))

    succeeded = 0
    for done, (address, (relative, targets)) in enumerate(tasks.items(), 1):
        try:
            bundle = addresses.get(address)
            if not bundle:
                raise ValueError("游戏资源目录中没有对应的素材包")
            environment = load_bundle(bundle)
            internal = addresses.asset_paths.get(address, address)
            image = probe.resolve_sprite(environment, internal).read().image
            image.thumbnail((max_size, max_size), Image.Resampling.LANCZOS)
            path = output / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            temporary = path.with_suffix(".tmp")
            image.save(temporary, format="PNG")
            temporary.replace(path)
            for cid, key in targets:
                if cid is None:
                    mapping["views"][key] = relative
                else:
                    mapping["characters"][cid]["portraits"][key] = relative
            succeeded += 1
        except Exception as error:
            failures.append({"address": address, "error": str(error)})
        if done == 1 or done % 10 == 0 or done == len(tasks):
            emit("progress", message="正在导出立绘、表情和背景", done=done, total=len(tasks), failed=len(failures))
    load_bundle.cache_clear()

    # Missing fallback portraits must remain visible as failures, never map to
    # a different character or a nonexistent image.
    unavailable = []
    for cid, character in list(mapping["characters"].items()):
        if character["first"] not in character["portraits"]:
            unavailable.append(cid)
            del mapping["characters"][cid]
    for name, color in [("black", "black"), ("white", "white")]:
        path = output / f"assets/views/{name}.png"
        path.parent.mkdir(parents=True, exist_ok=True)
        Image.new("RGB", (1920, 1080), color).save(path)
        mapping["views"][name] = f"assets/views/{name}.png"
    if not succeeded:
        details = list(dict.fromkeys(failure["error"] for failure in failures))[:3]
        raise ValueError("未能导出任何游戏图片，原素材库保持不变。具体原因：" + "；".join(details))
    report = {"exported": succeeded, "failed": len(failures), "failures": failures,
              "resource_platform": bundles.name,
              "unavailable_characters": unavailable,
              "characters": len(mapping["characters"]), "views": len(mapping["views"]),
              "game_tested": False}
    write_json(output / "preview_map.json", mapping)
    write_json(output / "extract_report.json", report)
    emit("result", **report)
    return report


def main():
    parser = argparse.ArgumentParser(description="从自己的游戏安装中提取编辑器预览图片")
    parser.add_argument("--self-check", action="store_true", help="只检查提取组件依赖，不读取游戏")
    parser.add_argument("--game-dir", type=Path)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--max-size", default=1024, type=int)
    args = parser.parse_args()
    if args.self_check:
        import UnityPy
        import _probe_assets
        from UnityPy.export import SpriteHelper, Texture2DConverter
        from PIL import Image
        from io import BytesIO
        Image.new("RGBA", (2, 2)).save(BytesIO(), format="PNG")
        emit("self-check", unitypy=UnityPy.__version__, pillow=Image.__version__,
             sprite_decoder_ready=callable(SpriteHelper.get_image_from_sprite), game_accessed=False)
        return 0
    if args.game_dir is None or args.output_dir is None:
        parser.error("请选择游戏目录和素材缓存目录")
    if not 128 <= args.max_size <= 1600:
        parser.error("图片尺寸必须在 128 到 1600 之间")
    try:
        extract(args.game_dir, args.output_dir, args.max_size)
    except Exception as error:
        emit("error", message=str(error))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
