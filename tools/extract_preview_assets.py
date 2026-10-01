#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""演出预览素材导出器：从游戏 Addressables bundle 批量导出立绘/背景图。

输入：
- data/editor_data.json          编辑器权威清单（人物/表情/view）
- data/assets/_probe/character_map.json  人物 id -> 表情名 -> addressKey（探测产物）
- data/assets/_probe/view_map.json       view 名 -> addressKey
- 游戏目录 catalog.json + StreamingAssets/aa/StandaloneWindows/*.bundle（只读）

输出：
- data/assets/portraits/<characterId>/<emotion>.png   立绘（同人物同 addressKey 只导一次）
- data/assets/views/<view>.png                        背景（别名共用贴图去重）
- data/assets/views/black.png / white.png             纯色图（Pillow 生成 1920x1080）
- data/preview_map.json                               编辑器读取的映射（回退已烘焙）
- data/assets/extract_report.json                     成功/失败统计

运行：C:/Users/mohui666/lom_unpack/venv/Scripts/python.exe tools/extract_preview_assets.py
"""
import json
import os
import sys
import argparse

import UnityPy
from PIL import Image

TOOLS_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS_DIR)
# _probe_assets 已将标准输出配置为 utf-8。
import _probe_assets as probe
from _probe_assets import parse_catalog, CATALOG, BUNDLE_DIR  # noqa: E402

DATA_DIR = os.path.normpath(os.path.join(TOOLS_DIR, "..", "data"))
EDITOR_DATA = os.path.join(DATA_DIR, "editor_data.json")
CHAR_MAP = os.path.join(DATA_DIR, "assets", "_probe", "character_map.json")
VIEW_MAP = os.path.join(DATA_DIR, "assets", "_probe", "view_map.json")
PORTRAIT_DIR = os.path.join(DATA_DIR, "assets", "portraits")
VIEW_DIR = os.path.join(DATA_DIR, "assets", "views")
PREVIEW_MAP = os.path.join(DATA_DIR, "preview_map.json")
REPORT = os.path.join(DATA_DIR, "assets", "extract_report.json")

SOLID_SIZE = (1920, 1080)

_bundle_cache = {}


def export_sprite(addr2bundle, address, out_path):
    """从 bundle 导出一张 Sprite 为 PNG。返回 None 表示成功，否则返回错误描述。"""
    bundle = addr2bundle.get(address)
    if bundle is None:
        return "catalog 无 bundle 映射"
    bpath = os.path.join(BUNDLE_DIR, bundle)
    if not os.path.isfile(bpath):
        return "bundle 文件缺失: " + bundle
    if bundle not in _bundle_cache:
        _bundle_cache[bundle] = UnityPy.load(bpath)
    try:
        internal_address = getattr(addr2bundle, "asset_paths", {}).get(address, address)
        sprite = probe.resolve_sprite(_bundle_cache[bundle], internal_address)
    except ValueError as exc:
        return str(exc)
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    sprite.read().image.save(out_path)
    return None


def main():
    global DATA_DIR, PORTRAIT_DIR, VIEW_DIR, PREVIEW_MAP, REPORT, CATALOG, BUNDLE_DIR
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--game-dir", help="包含 Mortal_Data 的本地游戏目录（只读）")
    parser.add_argument("--output-dir", help="私有预览目录，不会放进发布包")
    parser.add_argument("--max-size", type=int, default=0, help="预览图最长边像素；0 保留原图")
    parser.add_argument("--portraits-only", action="store_true")
    args = parser.parse_args()
    if args.max_size and not 128 <= args.max_size <= 1600:
        parser.error("--max-size 必须为 0 或 128~1600")
    if args.game_dir:
        game_data = os.path.join(os.path.abspath(args.game_dir), "Mortal_Data")
        probe.SA2 = os.path.join(game_data, "sharedassets2.assets")
        CATALOG = os.path.join(game_data, "StreamingAssets", "aa", "catalog.json")
        BUNDLE_DIR = os.path.join(game_data, "StreamingAssets", "aa", "StandaloneWindows")
    if args.output_dir:
        DATA_DIR = os.path.abspath(args.output_dir)
        PORTRAIT_DIR = os.path.join(DATA_DIR, "assets", "portraits")
        VIEW_DIR = os.path.join(DATA_DIR, "assets", "views")
        PREVIEW_MAP = os.path.join(DATA_DIR, "preview_map.json")
        REPORT = os.path.join(DATA_DIR, "assets", "extract_report.json")
    os.makedirs(os.path.join(DATA_DIR, "assets"), exist_ok=True)
    addr2bundle = parse_catalog(CATALOG)
    cm, vm = probe.load_sa2()
    ed = json.load(open(EDITOR_DATA, encoding="utf-8"))
    if "player_beautified" in cm:
        ed["characters"].append({"id": "player_beautified", "name": "美颜赵活", "portraits": list(cm["player_beautified"]["portraits"])})
    if args.portraits_only:
        ed["views"] = []
    print("[load] catalog=%d character_map=%d view_map=%d editor人物=%d editor场景=%d"
          % (len(addr2bundle), len(cm), len(vm), len(ed["characters"]), len(ed["views"])))

    report = {"portraits": {"success": 0, "failed": []},
              "views": {"success": 0, "failed": []},
              "solid": {"success": 0, "failed": []},
              "skipped_characters": []}

    # ---------- 立绘 ----------
    out_characters = {}
    export_total = 0
    export_done = 0
    # 预估总量用于进度显示
    for ch in ed["characters"]:
        cid = ch["id"]
        if cid in cm:
            export_total += len({cm[cid]["portraits"].get(e) or cm[cid]["first"] for e in ch["portraits"]})

    for ch in ed["characters"]:
        cid = ch["id"]
        if cid not in cm:
            report["skipped_characters"].append(cid)  # stone2/stone3/stone4/stones：不进 map
            continue
        conf = cm[cid]
        fallback_addr = conf.get("first")
        if not fallback_addr:
            report["skipped_characters"].append(cid)
            continue

        # address -> 文件名用表情名（editor 清单里首个指向它的表情）
        addr2emo = {}
        portraits_out = {}
        for emo in ch["portraits"]:
            addr = conf["portraits"].get(emo)
            if addr is None:
                continue  # 回退项最后统一处理
            if addr not in addr2emo:
                addr2emo[addr] = emo
            portraits_out[emo] = addr2emo[addr]

        # 回退目标文件：first 地址未被任何 editor 表情引用时，用配置里的表情名命名
        if fallback_addr not in addr2emo:
            cfg_emo = next((e for e, a in conf["portraits"].items() if a == fallback_addr), None)
            addr2emo[fallback_addr] = cfg_emo or "first"
        first_emo = addr2emo[fallback_addr]

        # 烘焙回退：editor 有但配置没有的表情 -> first 对应文件
        for emo in ch["portraits"]:
            if emo not in portraits_out:
                portraits_out[emo] = first_emo

        char_dir = os.path.join(PORTRAIT_DIR, cid)
        exported = {}  # 文件名表情 -> 相对路径
        for addr, emo in addr2emo.items():
            rel = "assets/portraits/%s/%s.png" % (cid, emo)
            out_path = os.path.join(char_dir, emo + ".png")
            if os.path.isfile(out_path):
                err = None  # 已存在则跳过导出（幂等重跑）
            else:
                err = export_sprite(addr2bundle, addr, out_path)
            if not err and args.max_size:
                with Image.open(out_path) as original:
                    if max(original.size) > args.max_size:
                        original.thumbnail((args.max_size, args.max_size), Image.Resampling.LANCZOS)
                        original.save(out_path)
            export_done += 1
            if err:
                report["portraits"]["failed"].append(
                    {"character": cid, "emotion": emo, "address": addr, "error": err})
            else:
                report["portraits"]["success"] += 1
                exported[emo] = rel
            if export_done % 100 == 0:
                print("[portraits] %d/%d (成功 %d, 失败 %d)" % (
                    export_done, export_total,
                    report["portraits"]["success"], len(report["portraits"]["failed"])))

        if first_emo in exported:
            out_characters[cid] = {
                "name": conf.get("name", cid),
                "first": first_emo,
                "portraits": {emo: exported[fn] for emo, fn in portraits_out.items() if fn in exported},
            }

    # ---------- 背景 ----------
    out_views = {}
    addr2view = {}  # address -> 文件名用 view 名（editor 清单里首个指向它的 view）
    for v in ed["views"]:
        if v in ("black", "white"):
            continue
        if v not in vm:
            report["views"]["failed"].append({"view": v, "address": None, "error": "view_map 无映射"})
            continue
        addr = vm[v]["address"]
        if addr not in addr2view:
            addr2view[addr] = v
        out_views[v] = addr2view[addr]

    view_total = len(addr2view)
    for i, (addr, v) in enumerate(addr2view.items(), 1):
        rel = "assets/views/%s.png" % v
        out_path = os.path.join(VIEW_DIR, v + ".png")
        err = None if os.path.isfile(out_path) else export_sprite(addr2bundle, addr, out_path)
        if err:
            report["views"]["failed"].append({"view": v, "address": addr, "error": err})
        else:
            report["views"]["success"] += 1
            out_views[v] = rel
        if i % 20 == 0 or i == view_total:
            print("[views] %d/%d (成功 %d, 失败 %d)" % (
                i, view_total, report["views"]["success"], len(report["views"]["failed"])))
    # 别名 view 指到同一相对路径（上面 out_views[v] 保存的是首个 view 名，这里统一换成 rel）
    # 说明：out_views 当前值为“文件名 view 名”，需转成实际路径
    for v, fname in list(out_views.items()):
        if isinstance(fname, str) and not fname.startswith("assets/"):
            rel = "assets/views/%s.png" % fname
            # 该文件导出失败则移除别名
            out_views[v] = rel
    for f in report["views"]["failed"]:
        out_views.pop(f["view"], None)
    # 去掉指向失败文件的别名
    failed_files = {"assets/views/%s.png" % f["view"] for f in report["views"]["failed"] if f["view"]}
    out_views = {v: p for v, p in out_views.items() if p not in failed_files}

    # ---------- black / white 纯色图 ----------
    for v, rgb in (("black", (0, 0, 0)), ("white", (255, 255, 255))):
        try:
            out_path = os.path.join(VIEW_DIR, v + ".png")
            if not os.path.isfile(out_path):
                os.makedirs(VIEW_DIR, exist_ok=True)
                Image.new("RGB", SOLID_SIZE, rgb).save(out_path)
            out_views[v] = "assets/views/%s.png" % v
            report["solid"]["success"] += 1
        except Exception as e:
            report["solid"]["failed"].append({"view": v, "error": repr(e)})

    # ---------- 写 preview_map.json / report ----------
    preview = {"characters": out_characters, "views": dict(sorted(out_views.items()))}
    with open(PREVIEW_MAP, "w", encoding="utf-8") as f:
        json.dump(preview, f, ensure_ascii=False, indent=1)
    with open(REPORT, "w", encoding="utf-8") as f:
        json.dump(report, f, ensure_ascii=False, indent=1)

    print("[done] characters=%d views=%d 立绘成功=%d 失败=%d 背景成功=%d 失败=%d 纯色成功=%d"
          % (len(out_characters), len(out_views),
             report["portraits"]["success"], len(report["portraits"]["failed"]),
             report["views"]["success"], len(report["views"]["failed"]),
             report["solid"]["success"]))
    print("[skipped] 无配置人物: %s" % report["skipped_characters"])
    print("[out] %s" % PREVIEW_MAP)
    print("[out] %s" % REPORT)


if __name__ == "__main__":
    main()
