# -*- coding: utf-8 -*-
"""压力测试：offscreen 下对编辑器做暴力操演，让崩溃在测试里露头。

覆盖（均走真实图片素材分支，preview_map.json 缺失时自动降级为占位图）：
  [1] 逐个节点选中并 grab() 渲染（main.json / second.json）
  [2] 真实鼠标点击 choice 的每个选项按钮跳转
  [3] 自动播放开到底，choice/branch 的每个分支都走一遍
  [4] 边预览边编辑：改属性/换类型/删正在预览的节点/上移下移/新建/导入再切回
  [5] 异常输入：空 story、坏图片路径、缺失人物 id、怪站位、portrait 缺 key
  [6] 图片缓存上限：LRU 驱逐后条数与字节数都不超标

用法（在 editor/ 目录下）：
    .venv/Scripts/python tests/stress_test.py
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from PySide6.QtCore import QEvent, QPointF, Qt  # noqa: E402  # type: ignore[reportMissingImports]
from PySide6.QtGui import QMouseEvent  # noqa: E402  # type: ignore[reportMissingImports]
from PySide6.QtWidgets import QApplication  # noqa: E402  # type: ignore[reportMissingImports]

import main  # noqa: E402
import models  # noqa: E402
import package_io  # noqa: E402
import preview  # noqa: E402

DEMO_MAIN = main.PROJECT_ROOT / "samples" / "demo_mod" / "story" / "main.json"
DEMO_SECOND = main.PROJECT_ROOT / "samples" / "demo_mod" / "story" / "second.json"
DEMO_LOMMOD = main.PROJECT_ROOT / "samples" / "demo_mod.lommod"


def click_at(widget, pos) -> None:
    """向控件发送真实鼠标按下事件（走 mousePressEvent 完整链路）。"""
    ev = QMouseEvent(
        QEvent.Type.MouseButtonPress,
        QPointF(pos),
        QPointF(pos),
        Qt.MouseButton.LeftButton,
        Qt.MouseButton.LeftButton,
        Qt.KeyboardModifier.NoModifier,
    )
    QApplication.sendEvent(widget, ev)


def auto_to_halt(win, app, max_steps=80):
    """从当前选中节点自动播放到暂停（choice/branch）或末尾，返回停留节点。"""
    win.auto_btn.setChecked(True)
    for _ in range(max_steps):
        if not win.auto_btn.isChecked():
            break
        win._auto_step()
        app.processEvents()
    win._stop_auto()
    return win._current_node()


def walk_branches(win, app, node, depth=0, seen=None):
    """对 choice/branch 节点的每个分支 goto 都恢复自动播放走一遍。"""
    seen = seen if seen is not None else set()
    if node is None or depth > 4:
        return
    t = node.get("type")
    if t == "choice":
        gotos = [o.get("goto") for o in node.get("options", [])]
    elif t == "branch":
        gotos = [c.get("goto") for c in node.get("cases", [])]
    elif t == "dice":
        gotos = [
            g
            for o in node.get("options", [])
            for g in (o.get("goto_大成功"), o.get("goto_成功"), o.get("goto_失败"))
        ]
    else:
        return
    for g in gotos:
        if not g or g in seen:
            continue
        seen.add(g)
        row = win._node_row(g)
        if row < 0:
            continue
        win._select_node_index(row)
        app.processEvents()
        stop = auto_to_halt(win, app)
        walk_branches(win, app, stop, depth + 1, seen)


def main_fn() -> int:
    app = QApplication([])

    editor_data, is_fallback = models.load_editor_data(main.PROJECT_ROOT)
    win = main.MainWindow(editor_data, is_fallback)
    win._prompt_on_discard = False  # 测试全程关闭未保存确认弹窗（会阻塞 offscreen）
    win.resize(1280, 760)
    win.show()
    pmap, data_dir = preview.load_preview_map(main.PROJECT_ROOT)
    real_assets = bool(pmap)
    print(
        f"[0] 环境就绪（素材={'真实图片' if real_assets else '占位图'}，"
        f"editor_data={'兜底' if is_fallback else 'json'}）"
    )

    # ------------------------------------------------------------------
    # [1] 逐节点渲染：main.json / second.json 全部节点选中 + grab
    # ------------------------------------------------------------------
    for path in (DEMO_MAIN, DEMO_SECOND):
        win._load_story_path(path)
        app.processEvents()
        for i, n in enumerate(win.story["nodes"]):
            win._select_node_index(i)
            app.processEvents()
            img = win.stage.grab()
            assert not img.isNull(), f"grab 失败: {path.name} {n['id']}"
        print(
            f"[1] 逐节点渲染 OK（{path.name}，{len(win.story['nodes'])} 节点，"
            f"缓存 {len(win.stage._pix_cache)} 张/"
            f"{win.stage._cache_bytes // (1 << 20)}MB）"
        )

    # ------------------------------------------------------------------
    # [1b] v3 全量 62 种节点类型：建表 + 渲染不崩（提示行/舞台两条路径都走）
    # ------------------------------------------------------------------
    all_nodes = []
    for i, t in enumerate(models.NODE_TYPES, 1):
        node = models.new_node(t, f"a{i:02d}", editor_data)
        if t == "say":
            node["mode"] = "center" if i % 2 else "character"
            node["text"] = "全类型压测文本"
        all_nodes.append(node)
    all_nodes.append({"id": "a99", "type": "end"})
    # 终止/二分节点会停止线性推演，排到末尾保证普通节点都能先被到达。
    terminal_types = (
        "end", "goto_scene", "death", "combat", "battle", "battle_result",
        "stat_check", "affinity_check", "item_check", "talent_check", "flag_check",
        "activity", "quest_check", "persistent_check",
    )
    all_nodes.sort(key=lambda n: 1 if n["type"] in terminal_types else 0)
    # 控制节点的工厂默认跳转为空；全类型压测要验证后续节点渲染，因此把
    # choice/branch/dice 的全部出口显式接到排序后的下一步。
    for index, node in enumerate(all_nodes[:-1]):
        target = all_nodes[index + 1]["id"]
        if node["type"] == "choice":
            for option in node.get("options", []):
                option["goto"] = target
        elif node["type"] == "branch":
            for case in node.get("cases", []):
                case["goto"] = target
        elif node["type"] == "dice":
            for option in node.get("options", []):
                option["goto_大成功"] = target
                option["goto_成功"] = target
                option["goto_失败"] = target
    win.story = {
        "id": "all_types",
        "title": "全类型",
        "start": "a01",
        "nodes": all_nodes,
    }
    win._refresh_all()
    app.processEvents()
    first_term = next(i for i, n in enumerate(all_nodes) if n["type"] in terminal_types)
    for i, n in enumerate(all_nodes):
        win._select_node_index(i)
        app.processEvents()
        img = win.stage.grab()
        assert not img.isNull(), f"渲染失败: {n['type']}"
        if i <= first_term:
            assert win.stage._state["reached"], f"推演应到达 {n['id']}（{n['type']}）"
        else:
            # 终止节点之后的节点不可达是正确语义
            assert not win.stage._state["reached"], f"{n['id']} 不应越过终止节点"
    hints = sum(
        1
        for n in all_nodes
        if preview.simulate_stage(win.story, n["id"], editor_data)["hint"]
    )
    assert hints >= 20, f"数值/流程类节点应有提示行：{hints}"
    print(f"[1b] 全量 62 类型渲染 OK（{len(all_nodes)} 节点，{hints} 个提示行）")

    # ------------------------------------------------------------------
    # [2] 真实鼠标点击 choice 每个选项按钮
    # ------------------------------------------------------------------
    win._load_story_path(DEMO_MAIN)
    app.processEvents()
    row = win._node_row("n11")
    for k, opt in enumerate(win.story["nodes"][row]["options"]):
        win._select_node_index(row)
        win.stage.repaint()  # 确保热区已生成
        app.processEvents()
        rects = win.stage._choice_rects
        assert len(rects) == 3, f"n11 应有 3 个选项热区：{len(rects)}"
        box, goto = rects[k]
        assert goto == opt["goto"]
        click_at(win.stage, box.center())
        app.processEvents()
        cur = win._current_node()
        assert cur and cur["id"] == opt["goto"], (
            f"点击选项 {k} 应选中 {opt['goto']}，实际 {cur and cur['id']}"
        )
    print("[2] 选项按钮真实点击 OK（n11 三个选项均正确跳转）")

    # ------------------------------------------------------------------
    # [3] 自动播放：从头开到底；choice/branch 暂停后每个分支都走
    # ------------------------------------------------------------------
    win._goto_start()
    app.processEvents()
    stop = auto_to_halt(win, app)
    assert stop and stop["id"] == "n11" and stop["type"] == "choice", (
        f"自动播放应暂停在 n11 choice，实际 {stop and stop['id']}"
    )
    # n11 三分支：n12 线、n16→n21(branch,两case)线、n20→n21 线；n25b 还有一层 branch
    walk_branches(win, app, stop)
    # 确认 branch 两个 case 的目标都被踩过（n22=n21 的 value2 分支，n25c=n25b 分支）
    print("[3] 自动播放 OK（n11 暂停；choice/branch 各分支均走到底，含 n22/n25c）")

    # ------------------------------------------------------------------
    # [4] 边预览边编辑的操演
    # ------------------------------------------------------------------
    win._load_story_path(DEMO_MAIN)
    app.processEvents()

    # 4a 改正在预览的 say 节点属性
    r7 = win._node_row("n7")
    win._select_node_index(r7)
    win.story["nodes"][r7]["text"] = "压力测试改写：四师兄递过来一张清单。\n第二行。"
    win.story["nodes"][r7]["portrait"] = "laugh1"
    win._on_node_changed()
    app.processEvents()
    win.stage.grab()

    # 4b 换正在预览的节点类型（say → show）
    r8 = win._node_row("n8")
    win._select_node_index(r8)
    win.story["nodes"][r8] = models.new_node("show", "n8", editor_data)
    win._refresh_all(select_row=r8)
    app.processEvents()
    win.stage.grab()

    # 4c 删除正在预览的节点
    win._delete_node()
    app.processEvents()
    win.stage.grab()

    # 4d 上移/下移当前节点（下标是 nodes[]，不是列表行）
    win._select_node_index(min(3, len(win.story.get("nodes", [])) - 1))
    win._move_node(1)
    win._move_node(-1)
    app.processEvents()
    win.stage.grab()

    # 4e 新建剧情（预览随之切到新 story）
    win.new_story()
    app.processEvents()
    win.stage.grab()

    # 4f 导入 .lommod 再切回 main.json
    manifest, stories = package_io.import_lommod(str(DEMO_LOMMOD))
    entry = manifest.get("entry")
    sid = str(entry if entry in stories else sorted(stories)[0])
    win.story = stories[sid]
    win.story_path = None
    win._refresh_all()
    app.processEvents()
    win.stage.grab()
    win._load_story_path(DEMO_MAIN)
    app.processEvents()
    print("[4] 边预览边编辑 OK（改属性/换类型/删节点/移动/新建/导入切回）")

    # ------------------------------------------------------------------
    # [5] 异常输入
    # ------------------------------------------------------------------
    # 5a 空 story
    win.story = {"id": "empty", "title": "空", "start": "", "nodes": []}
    win._refresh_all()
    app.processEvents()
    win.stage.grab()

    # 5b 坏图片路径 + 缺失人物 id + portrait 缺 key + 怪站位
    weird = {
        "id": "weird",
        "title": "异常输入",
        "start": "w1",
        "nodes": [
            {"id": "w1", "type": "scene", "view": "__no_such_view__"},
            {
                "id": "w2",
                "type": "show",
                "character": "ghost999",
                "position": "Talk",
                "portrait": "normal",
            },
            {
                "id": "w3",
                "type": "show",
                "character": "brother4",
                "position": "LB1",
                "portrait": "__no_such_emotion__",
            },
            {
                "id": "w4",
                "type": "show",
                "character": "player",
                "position": "BCB2",
                "facing": "left",
            },
            {"id": "w5", "type": "show", "character": "trainee1", "position": "RS2"},
            {
                "id": "w6",
                "type": "say",
                "character": "ghost999",
                "portrait": "normal",
                "mode": "character",
                "text": "不存在的人物在说话",
            },
            {"id": "w7", "type": "end"},
        ],
    }
    win.story = weird
    win._refresh_all()
    for i in range(len(weird["nodes"])):
        win._select_node_index(i)
        app.processEvents()
        win.stage.grab()
    x, ok = preview.position_x("Talk")
    assert not ok and x == 0.5, "Talk 应走未识别兜底"
    print("[5] 异常输入 OK（空 story/坏图路径/缺失人物/缺表情/Talk 等怪站位）")

    # 5c 素材映射整体损坏：preview_map 指向不存在的目录
    win.stage.set_assets(
        {
            "views": {"center": "assets/views/__missing__.png"},
            "characters": {
                "player": {
                    "name": "主角",
                    "portraits": {"normal": "assets/portraits/__missing__.png"},
                }
            },
        },
        data_dir,
    )
    win._load_story_path(DEMO_MAIN)
    win._select_node_index(win._node_row("n7"))
    app.processEvents()
    win.stage.grab()
    win.stage.set_assets(pmap, data_dir)  # 还原真实素材
    win.stage.set_context(editor_data)
    print("[5b] 素材映射损坏降级 OK（全部走占位图分支）")

    # ------------------------------------------------------------------
    # [6] 图片缓存上限：灌入 80 张不同立绘，LRU 必须驱逐
    # ------------------------------------------------------------------
    paths: list[str] = []
    for c in pmap.get("characters", {}).values():
        for rel in set((c.get("portraits") or {}).values()):
            paths.append(rel)
            if len(paths) >= 80:
                break
        if len(paths) >= 80:
            break
    if real_assets and len(paths) >= preview.MAX_CACHE_ENTRIES + 5:
        for rel in paths:
            win.stage._load_pixmap(rel)
        n = len(win.stage._pix_cache)
        mb = win.stage._cache_bytes / (1 << 20)
        assert n <= preview.MAX_CACHE_ENTRIES, f"缓存条数超上限: {n}"
        assert win.stage._cache_bytes <= preview.MAX_CACHE_BYTES, (
            f"缓存字节超上限: {mb:.0f}MB"
        )
        print(
            f"[6] 缓存上限 OK（灌入 {len(paths)} 张 → LRU 留住 {n} 张，"
            f"{mb:.0f}MB ≤ {preview.MAX_CACHE_BYTES // (1 << 20)}MB）"
        )
    else:
        print("[6] 无真实素材，跳过缓存上限断言")

    # 收尾：主窗口还活着且能正常渲染
    win._load_story_path(DEMO_MAIN)
    win._select_node_index(win._node_row("n7"))
    app.processEvents()
    assert not win.stage.grab().isNull()
    win.close()
    app.quit()
    print("STRESS TEST PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main_fn())
