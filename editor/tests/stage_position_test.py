# -*- coding: utf-8 -*-
"""舞台站位表（data/stage_positions.json + preview 兜底值）的回归测试。

用法（在 editor/ 目录下）：
    .venv/Scripts/python tests/stage_position_test.py

背景：原预览只硬编码了 36 个站位里的 14 个，且不少值/顺序是错的（如 R3=0.70 排在
R2=0.78 左边、LM1 被当成中间偏右）。权威值从游戏剧情场景（Mortal_Data/level2）的
站位锚点提取：x 由数字 1/2/3 决定左右列，feet 是「角色脚底距画面顶部比例」，
基准行脚底正好站在画面底边，S/M/B 行依次更低。
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

import preview  # noqa: E402
import models  # noqa: E402


def test_table_covers_all_positions() -> None:
    table = preview.load_stage_positions(preview.PROJECT_ROOT)
    editor_ids = {str(p["id"]).upper() for p in models.load_editor_data(preview.PROJECT_ROOT)[0].get("positions", [])}
    missing = editor_ids - set(table) - {"C"}  # C 是编辑器别名，非场景锚点
    assert not missing, f"站位表缺少这些编辑器站位：{sorted(missing)}"
    # 场景里还可能有编辑器没列出的（如 Talk），不要求完全相等，只要求覆盖编辑器
    print(f"[覆盖] 站位表 {len(table)} 项，覆盖编辑器全部 {len(editor_ids)} 个站位")


def test_x_matches_authoritative_scene() -> None:
    cases = {
        "L1": 0.145313, "L3": 0.353646, "M": 0.5,
        "R1": 0.646354, "R3": 0.854688,
        "SL": -0.5, "SR": 1.5, "Talk": 0.253646,
        "LM1": 0.145313,  # 与 L1 同列（数字决定 x，字母决定纵深）
    }
    for name, want in cases.items():
        x, known = preview.position_x(name)
        assert known, f"{name} 应被识别"
        assert abs(x - want) < 1e-4, f"{name} x 应为 {want}，实际 {x}"
    print("[x 坐标] 抽查全部与场景权威值一致")


def test_feet_depth_order() -> None:
    # 同一列不同纵深：基准行 < S < M < B（脚底越靠后越往下）
    front = preview.position_feet("L1")
    assert preview.position_feet("LS1") > front
    assert preview.position_feet("LM1") > preview.position_feet("LS1")
    assert preview.position_feet("LB1") > preview.position_feet("LM1")
    # 左右对称：同数字的左右站位列 feet 相同
    assert abs(preview.position_feet("L2") - preview.position_feet("R2")) < 1e-9
    assert abs(preview.position_feet("LM3") - preview.position_feet("RM3")) < 1e-9
    print("[纵深] 基准 < 屏外 < 中 < 后，左右对称")


def test_json_matches_fallback() -> None:
    path = preview.PROJECT_ROOT / "data" / "stage_positions.json"
    assert path.is_file(), "data/stage_positions.json 不存在"
    data = json.loads(path.read_text(encoding="utf-8"))
    positions = {str(k).upper(): v for k, v in data["positions"].items()}
    table = preview.load_stage_positions(preview.PROJECT_ROOT)
    for name, (x, feet) in table.items():
        if name == "C":
            continue
        assert name in positions, f"JSON 缺少 {name}"
        assert abs(positions[name]["x"] - x) < 1e-5, f"{name} x 不一致"
        assert abs(positions[name]["feet"] - feet) < 1e-5, f"{name} feet 不一致"
    print("[一致性] JSON 与内置兜底值逐项一致")


def main_fn() -> int:
    test_table_covers_all_positions()
    test_x_matches_authoritative_scene()
    test_feet_depth_order()
    test_json_matches_fallback()
    print("\nstage_position_test 全部通过")
    return 0


if __name__ == "__main__":
    sys.exit(main_fn())
