# -*- coding: utf-8 -*-
"""从游戏剧情场景提取「舞台站位」锚点坐标，生成 data/stage_positions.json。

为什么需要这个文件：
  编辑器预览里把站位画成横向一排在一条基线上，而且只硬编码了 36 个站位中的
  14 个（其余兜底画到正中央）。真实游戏里站位是场景里的一组 RectTransform 锚点，
  横坐标由数字决定（1/2/3 → 左/右列），纵坐标是「角色脚底站位点」——越靠后的
  行越低。

依据（反编译 + 场景实测，2026-10）：
  - 角色位置由 Fungus 的 Stage.GetPosition(name) 按名字查 positions 列表
    （Fungus/Stage.cs），PortraitController.SetRectTransform 把锚点的
    anchoredPosition / sizeDelta / anchor / pivot 整套复制给角色容器。
  - 锚点统一 anchorMin=anchorMax=(0.5,1.0)、pivot=(0.5,0.0)：即「脚底点」在
    父 Canvas 顶部中心 + anchoredPosition 处。
  - 因此 x 比例 = 0.5 + anchoredPosition.x / 1920；
    脚底距画面顶部像素 = -anchoredPosition.y（1080 参考高）。

输出：
  data/stage_positions.json = { "schema": 1, "reference": {...},
    "positions": { "<id>": {"x": 0..1(可越界), "feet": 顶部起比例} } }
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

try:
    import UnityPy  # type: ignore[reportMissingImports]
except Exception:  # noqa: BLE001
    UnityPy = None  # type: ignore[reportAssignmentType]

ROOT = Path(__file__).resolve().parent.parent

# 参考分辨率：锚点 x 用 1920 归一，纵坐标用 1080 归一。
REF_WIDTH = 1920.0
REF_HEIGHT = 1080.0

# 站位锚点的特征签名：顶部中心锚点 + 底边中心轴心
SIG = ((0.5, 1.0), (0.5, 1.0), (0.5, 0.0))


def _rect_objects(env):
    rts = {}
    go_name = {}
    for obj in env.objects:
        if obj.type.name == "GameObject":
            try:
                go_name[obj.path_id] = obj.read().m_Name
            except Exception:  # noqa: BLE001
                pass
        elif obj.type.name == "RectTransform":
            try:
                rts[obj.path_id] = obj.read()
            except Exception:  # noqa: BLE001
                pass
    return rts, go_name


def _canvas_parent(rts):
    """定位「所有站位锚点的共同父级」：名字为 M 或 L1 的锚点的父亲。"""
    for pid, d in rts.items():
        if getattr(d.m_GameObject, "path_id", 0):
            pass
    # 用 m_Name == "M" 的 GameObject 对应 RectTransform 的父亲
    by_name = {}
    for pid, d in rts.items():
        go = getattr(d.m_GameObject, "path_id", 0)
        if go:
            by_name.setdefault(go, pid)
    # 需要 GameObject 名 -> path_id
    return None


def extract(level_path: Path) -> dict:
    if UnityPy is None:
        raise SystemExit("缺少 UnityPy，先安装：pip install UnityPy")
    env = UnityPy.load(str(level_path))
    rts, go_name = _rect_objects(env)

    # GameObject 名 -> 其 RectTransform
    name2rt = {}
    for rt_pid, d in rts.items():
        go = getattr(d.m_GameObject, "path_id", 0)
        name = go_name.get(go)
        if name:
            name2rt[name] = rt_pid

    # 站位锚点父级：锚点 "M" 的父亲
    anchor_names = {"M", "L1", "R1", "SL", "MM", "LB1"}
    canvas_pid = None
    for nm in anchor_names:
        rt_pid = name2rt.get(nm)
        if rt_pid is None:
            continue
        d = rts[rt_pid]
        fp = getattr(d.m_Father, "path_id", 0)
        if fp in rts:
            canvas_pid = fp
            break
    if canvas_pid is None:
        raise SystemExit("找不到站位锚点的父 Canvas")

    positions: dict[str, dict] = {}
    for rt_pid, d in rts.items():
        if getattr(d.m_Father, "path_id", 0) != canvas_pid:
            continue
        sig = (
            (float(d.m_AnchorMin.x), float(d.m_AnchorMin.y)),
            (float(d.m_AnchorMax.x), float(d.m_AnchorMax.y)),
            (float(d.m_Pivot.x), float(d.m_Pivot.y)),
        )
        if sig != SIG:
            continue
        name = go_name.get(getattr(d.m_GameObject, "path_id", 0))
        if not name:
            continue
        ap_x = float(d.m_AnchoredPosition.x)
        ap_y = float(d.m_AnchoredPosition.y)
        positions[name] = {
            "x": round(0.5 + ap_x / REF_WIDTH, 6),
            # 脚底距画面顶部（1080 参考）：Canvas 顶部中心 + anchoredPosition.y
            "feet": round(-ap_y / REF_HEIGHT, 6),
        }

    return positions


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("level", nargs="?", help="游戏 Mortal_Data/level2 路径")
    parser.add_argument("--out", default=str(ROOT / "data" / "stage_positions.json"))
    args = parser.parse_args(argv)

    if not args.level:
        parser.error("需要传入游戏场景路径，例如 D:/.../LegendOfMortal/Mortal_Data/level2")

    positions = extract(Path(args.level))
    if not positions:
        raise SystemExit("没有提取到任何站位锚点")

    payload = {
        "schema": 1,
        "note": (
            "由 tools/extract_stage_positions.py 从游戏剧情场景提取（脚底站位点）。"
            "x=横向比例(0=左缘,1=右缘，可越界表示屏外)；feet=脚底距画面顶部比例(1080 参考)。"
        ),
        "reference": {"width": REF_WIDTH, "height": REF_HEIGHT},
        "positions": dict(sorted(positions.items())),
    }
    out = Path(args.out)
    out.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"[OK ] 提取 {len(positions)} 个站位 -> {out}")
    for name, val in sorted(positions.items()):
        print(f"     {name:<6} x={val['x']:<8} feet={val['feet']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
