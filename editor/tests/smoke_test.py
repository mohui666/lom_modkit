# -*- coding: utf-8 -*-
"""冒烟测试：offscreen 实例化主窗口 → 新建节点 → 切换节点类型 → 序列化往返。

用法（在 editor/ 目录下）：
    .venv/Scripts/python tests/smoke_test.py
"""
from __future__ import annotations

import copy
import os
import sys
import tempfile
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from PySide6.QtCore import QTimer  # noqa: E402  # type: ignore[reportMissingImports]
from PySide6.QtWidgets import QApplication, QMessageBox  # noqa: E402  # type: ignore[reportMissingImports]

import main  # noqa: E402
import models  # noqa: E402


def main_fn() -> int:
    app = QApplication([])

    editor_data, is_fallback = models.load_editor_data(main.PROJECT_ROOT)
    win = main.MainWindow(editor_data, is_fallback)
    win._prompt_on_discard = False  # 测试全程关闭未保存确认弹窗（会阻塞 offscreen）
    assert win.node_list.count() == 1, "新建剧情应含 1 个起始节点"
    print(f"[1] 主窗口实例化 OK（{'兜底数据' if is_fallback else 'editor_data.json'}，"
          f"节点数={win.node_list.count()}）")

    # 新建节点：在起始节点后插入一个 choice
    win._add_node("choice")
    assert win.node_list.count() == 2
    node = win._current_node()
    assert node is not None and node["type"] == "choice" \
        and len(node["options"]) == 2, "choice 默认 2 个选项"
    print(f"[2] 新建节点 OK（{node['id']}，summary={models.node_summary(node, editor_data)!r}）")

    # 切换节点类型：当前 choice → 换成 branch 工厂产物，并验证表单按 type 重建
    branch = models.new_node("branch", node["id"], editor_data)
    branch["flag"] = "SMOKE_FLAG"
    branch["cases"][0]["goto"] = "n1"  # 指向已有节点，保证 story 合法可编译
    win.story["nodes"][win.node_list.currentRow()] = branch
    win._refresh_all(select_row=1)
    cur = win._current_node()
    assert cur is not None and cur["type"] == "branch"
    assert len(branch["cases"]) >= 1, "branch 至少 1 个 case"
    # 契约更新：branch 新节点默认 source="mod"，摘要带 source 与 flag
    assert branch["source"] == "mod"
    summary = models.node_summary(branch, editor_data)
    assert "本mod:SMOKE_FLAG" in summary, f"branch 摘要应含中文来源与 flag：{summary!r}"
    assert summary.startswith("条件分支·"), f"摘要应带中文类型名：{summary!r}"
    # 表单应显示 branch 字段而非 choice 字段
    assert win.form._node is branch
    print(f"[3] 切换节点类型 OK（choice → branch，summary={summary!r}）")

    # branch.source 切换：game → mod 时非法 value/超行被归一（契约：仅 1/2、≤2 行）
    from PySide6.QtWidgets import QComboBox  # type: ignore[reportMissingImports]
    branch["source"] = "game"
    branch["cases"] = [{"value": 7, "goto": "n1"}, {"value": 1, "goto": "n1"},
                       {"value": 2, "goto": "n1"}, {"value": 3, "goto": "n1"}]
    win._refresh_all(select_row=1)
    cb = QComboBox()
    cb.addItem("mod", "mod")
    cb.setCurrentIndex(0)
    win.form._on_source_changed(branch, "source", cb)
    vals = [c["value"] for c in branch["cases"]]
    assert branch["source"] == "mod" and vals == [1, 2], f"归一化失败: {vals}"
    print(f"[3b] branch.source game→mod 归一化 OK（cases value={vals}）")

    # 表单写回：改 say 文本后摘要应更新
    win.node_list.setCurrentRow(0)
    say_node = win._current_node()
    assert say_node is not None
    say_node["text"] = "这是一段用于冒烟测试的对话文本，长度超过二十个字用于验证截断"
    win._on_node_changed()
    text = win.node_list.item(0).text()
    assert "…" in text, f"摘要应截断：{text!r}"
    print(f"[4] 摘要刷新 OK（{text!r}）")

    # 序列化往返：save → load 内容一致
    with tempfile.TemporaryDirectory() as tmp:
        p = Path(tmp) / "story.json"
        models.save_story(win.story, p)
        try:
            back = models.load_story(p)
        except Exception as exc:
            raise AssertionError(f"load_story 失败: {exc}") from exc
    assert copy.deepcopy(win.story) == back, "序列化往返不一致"
    print(f"[5] story.json 序列化往返 OK（{len(back['nodes'])} 节点）")

    # 兜底数据完整性（契约 §5 关键键）
    for key in ("characters", "positions", "modes", "stats"):
        assert key in editor_data, f"editor_data 缺 {key}"
    ids = {c["id"] for c in editor_data["characters"]}
    assert {"player", "brother4", "trainee1"} <= ids, "兜底人物不全"
    print("[6] editor_data 关键键/兜底人物 OK")

    # lomc 集成：预览编译当前文档；再把 goto 指向不存在节点应报错
    import lua_preview
    lua, err = lua_preview.compile_story(win.story)
    if lua_preview.lomc_available():
        assert err is None and lua and "function" in lua, f"编译失败: {err}"
        win.story["nodes"][0]["goto"] = "not_exist"
        _lua2, err2 = lua_preview.compile_story(win.story)
        assert err2, "悬空 goto 应报编译错误"
        del win.story["nodes"][0]["goto"]
        print(f"[7] lomc 实时编译 OK（lua {len(lua)} 字符；悬空 goto 正确报错）")
    else:
        assert err and "不可用" in err
        print(f"[7] lomc 不可用，红字降级路径 OK（{err.splitlines()[0]}）")

    # .lommod 导出→导入往返（走 package_io，lomc 可用时经官方 pack）
    import package_io
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "smoke.lommod"
        campaign = {"new_game": True, "triggers": [
            {"type": "position", "position": "Center", "script": win.story["id"],
             "when_flag_set": "SMOKE_FLAG"}]}
        manifest = {"format": 1, "id": "smoke_mod", "name": "冒烟",
                    "version": "1.0.0", "author": "test",
                    "description": "冒烟测试用包", "entry": win.story["id"],
                    "campaign": campaign}
        try:
            report = package_io.export_lommod(out, manifest,
                                              {win.story["id"]: win.story})
        except package_io.PackError as exc:
            if lua_preview.lomc_available():
                raise AssertionError(f"导出失败: {exc}")
            print(f"[8] lomc 不可用，导出按预期报错降级（{str(exc).splitlines()[0]}）")
        else:
            assert out.exists() and report
            m2, stories2 = package_io.import_lommod(out)
            assert m2["id"] == "smoke_mod" and win.story["id"] in stories2
            back_story = stories2[win.story["id"]]
            assert back_story["nodes"] == win.story["nodes"], "包内 story 与编辑状态不一致"
            assert m2.get("campaign") == campaign, \
                f"campaign 往返不一致：{m2.get('campaign')!r}"
            print(f"[8] .lommod 导出→导入往返 OK（{out.stat().st_size} 字节，"
                  f"campaign 往返一致）")

    # ------------------------------------------------------------------
    # v3：汉化覆盖 + schema 2 + 新节点表单 + manifest campaign 对话框
    # ------------------------------------------------------------------
    # 38 种节点类型中文名全覆盖；菜单分组与类型表一一对应
    assert len(models.NODE_TYPES) == 38, f"契约 v3 应有 38 种节点：{len(models.NODE_TYPES)}"
    assert set(models.NODE_TYPE_CN) == set(models.NODE_TYPES), "NODE_TYPE_CN 未全覆盖"
    grouped = [t for _g, ts in models.NODE_GROUPS for t in ts]
    assert sorted(grouped) == sorted(models.NODE_TYPES), "NODE_GROUPS 与类型表不一致"
    assert [g for g, _t in models.NODE_GROUPS] == ["演出类", "数值状态类", "流程类"]

    # schema 2 清单助手：{id,name} 显示 "名字（id）"，兼容字符串与缺键
    assert models.entry_display({"id": "mental", "name": "心相"}) == "心相（mental）"
    assert models.entry_display({"id": "x", "name": "x"}) == "x"
    assert models.entry_display("plain") == "plain"
    assert models.list_items(editor_data, "stats"), "stats 清单应非空"
    assert models.list_items(editor_data, "不存在的键") == []
    # 真实数据抽查：人物/站位/属性清单均为对象数组且能查到中文名
    for key in ("characters", "positions", "stats", "views", "music"):
        items = editor_data.get(key) or []
        assert items and isinstance(items[0], dict), f"{key} 应为 schema 2 对象数组"
    assert models.display_name(editor_data, "stats", "mental") == "心相"

    # 中文摘要抽查
    s = models.node_summary({"id": "x", "type": "say", "mode": "center",
                             "text": "居中"}, editor_data)
    assert s.startswith("对白·居中旁白"), s
    s = models.node_summary({"id": "x", "type": "dice", "check": "C1",
                             "options": [{"text": "a"}]}, editor_data)
    assert s == "骰子检定·C1(1项)", s
    s = models.node_summary({"id": "x", "type": "goto_scene", "scene": "Combat",
                             "key": "5102_01"}, editor_data)
    assert s == "场景跳转·战斗 5102_01", s
    print("[8b] 汉化/schema 2 助手抽查 OK（38 类型中文名、三组分组、清单显示）")

    # ManifestDialog：campaign 区读写 + 空行跳过 + 无内容不写出
    dlg = main.ManifestDialog(
        "main", editor_data, ["main", "second"],
        {"campaign": {"new_game": True, "triggers": [
            {"type": "position", "position": "Center", "script": "second",
             "when_flag_set": "F1"}]}})
    assert dlg.new_game_check.isChecked(), "应回填 new_game"
    assert dlg.triggers_table.rowCount() == 1, "应回填 1 行触发器"
    m = dlg.manifest()
    assert m["campaign"]["new_game"]
    trig = m["campaign"]["triggers"][0]
    assert trig["position"] == "Center" and trig["script"] == "second" \
        and trig["when_flag_set"] == "F1", f"触发器回填错误：{trig!r}"
    dlg._add_trigger_row({})  # 空行：位置/脚本缺失，应被跳过
    assert len(dlg.manifest()["campaign"]["triggers"]) == 1
    dlg.new_game_check.setChecked(False)
    dlg._del_trigger_row()
    dlg._del_trigger_row()
    assert not dlg.manifest().get("campaign"), "无 new_game 且无有效触发器不应写 campaign"
    print("[8c] ManifestDialog campaign 区 OK（回填/跳过空行/空则不写）")

    # 新节点类型：建表 → 表单构建 → 字段写回 → 预览渲染不崩
    win.new_story()
    new_types = ["sound", "offset", "effect", "transition", "camera", "block",
                 "cg", "stat_set", "talent", "item", "game_flag", "enemy",
                 "battle_skill", "mission", "time", "autosave", "dice",
                 "goto_scene", "panel", "raw"]
    for t in new_types:
        win._add_node(t)
        node = win._current_node()
        assert node is not None and node["type"] == t, f"新增 {t} 失败"
        assert win.form._node is node, f"{t} 表单未构建"
        win.stage.grab()  # 演出预览渲染（提示行分支）
    # 字段写回抽查：raw 代码框 / panel 枚举 / dice 阈值
    win.node_list.setCurrentRow(win._node_row(
        [n for n in win.story["nodes"] if n["type"] == "raw"][0]["id"]))
    raw_node = win._current_node()
    assert raw_node is not None
    win.form.set_node(raw_node)
    from PySide6.QtWidgets import QPlainTextEdit  # type: ignore[reportMissingImports]
    code_edit = next(w for w in win.form.findChildren(QPlainTextEdit)
                     if w.property("code_edit"))
    code_edit.setPlainText("say(\"hi\")\nwait(1)")
    assert raw_node["code"] == 'say("hi")\nwait(1)', "raw 代码写回失败"
    panel_node = next(n for n in win.story["nodes"] if n["type"] == "panel")
    win.form.set_node(panel_node)
    from PySide6.QtWidgets import QComboBox as _CB  # type: ignore[reportMissingImports]
    panel_combo = next(c for c in win.form.findChildren(_CB)
                       if c.currentData() == "martial")
    idx = panel_combo.findData("shop")
    panel_combo.setCurrentIndex(idx)
    assert panel_node["panel"] == "shop", "panel 枚举写回失败"
    dice_node = next(n for n in win.story["nodes"] if n["type"] == "dice")
    dice_node["options"][0]["threshold"] = 66
    win.form.set_node(dice_node)
    assert "骰子检定" in models.node_summary(dice_node, editor_data)
    print(f"[8d] 新节点类型 OK（{len(new_types)} 种建表+表单+渲染；"
          f"raw/panel/dice 写回抽查通过）")

    # ------------------------------------------------------------------
    # 演出预览：demo 剧情推演 + 渲染 + 抓图（素材缺失时走占位图分支）
    # ------------------------------------------------------------------
    import preview
    pmap, data_dir = preview.load_preview_map(main.PROJECT_ROOT)
    print(f"[9] preview_map {'已加载' if pmap else '缺失，走占位图'}（{data_dir}）")

    demo_path = main.PROJECT_ROOT / "samples" / "demo_mod" / "story" / "main.json"
    demo = models.load_story(demo_path)
    # 推演：关键节点均应到达且不崩
    sims = {nid: preview.simulate_stage(demo, nid)
            for nid in ("n1", "n2", "n7", "n11", "n23")}
    for nid, st in sims.items():
        assert st["reached"], f"推演应到达 {nid}（steps={st['steps']}）"
    # n2 切完 center 场景
    assert sims["n2"]["view"] == "center", f"n2 view 应为 center：{sims['n2']['view']}"
    # n7 brother4 说话：对白栏 + 台上三人（player@M / brother4@R2 / trainee1@L2）
    d7 = sims["n7"]["dialog"]
    assert d7 and d7["character"] == "brother4" and d7["mode"] == "character"
    a7 = sims["n7"]["actors"]
    assert a7.get("brother4", {}).get("position") == "R2" \
        and a7["brother4"]["facing"] == "left", f"n7 brother4 状态异常：{a7}"
    assert a7.get("trainee1", {}).get("portrait") == "nervous1"
    # n11 三选一：choice 三个选项
    c11 = sims["n11"]["choice"]
    assert c11 and len(c11) == 3 and c11[0]["goto"] == "n12", f"n11 选项异常：{c11}"
    # n23 hide trainee1：场上只剩 player 和 brother4
    a23 = sims["n23"]["actors"]
    assert "trainee1" not in a23 and {"player", "brother4"} <= set(a23), \
        f"n23 应已隐藏 trainee1：{a23}"
    print(f"[10] demo 推演 OK（n2 场景={sims['n2']['view']}；n7 对白="
          f"{models.character_name(editor_data, d7['character'])}；n11 {len(c11)} 选项；"
          f"n23 台上={sorted(a23)}）")

    # 渲染：主窗口载入 demo，逐节点渲染不崩
    win._load_story_path(demo_path)
    assert win.right_tabs.count() == 2, "右栏应为演出预览/Lua 两个页签"
    for nid in ("n1", "n2", "n7", "n11", "n23"):
        row = win._node_row(nid)
        win.node_list.setCurrentRow(row)
        assert win.stage._state["reached"], f"主窗口预览 {nid} 应到达"
        win.stage.repaint()  # offscreen 下强制重绘，验证 paintEvent 不崩
    print("[11] 主窗口集成 OK（演出预览/Lua 页签、选中刷新、paintEvent 无异常）")

    # 抓图：n2（场景 center）/ n7（brother4 说话）/ n11（三选一）各存一张 PNG
    out_dir = EDITOR_DIR / "tests" / "out"
    out_dir.mkdir(parents=True, exist_ok=True)
    win.resize(1280, 760)
    win.show()
    win.right_tabs.setCurrentIndex(0)
    for nid in ("n2", "n7", "n11"):
        win.node_list.setCurrentRow(win._node_row(nid))
        app.processEvents()
        img = win.stage.grab()
        out = out_dir / f"preview_{nid}.png"
        assert img.save(str(out)), f"抓图保存失败：{out}"
        assert out.exists() and out.stat().st_size > 0
        print(f"[12] 抓图 OK：{out}（{out.stat().st_size} 字节）")

    # 选项点击交互：模拟点击 n11 第一个选项 → 主窗口应选中 n12
    win.node_list.setCurrentRow(win._node_row("n11"))
    win.stage.repaint()  # 强制重绘以生成选项热区
    app.processEvents()
    assert win.stage._choice_rects, "n11 应有选项按钮热区"
    win.stage.choice_activated.emit(win.stage._choice_rects[0][1])
    app.processEvents()
    n12_node = win._current_node()
    assert n12_node is not None and n12_node["id"] == "n12", \
        "点击选项应跳转到 goto 节点 n12"
    print("[13] 选项点击跳转 OK（n11 → n12）")

    # 步进与自动播放：自动播放应在 choice 处自动暂停
    win._goto_start()
    start_node = win._current_node()
    assert start_node is not None and start_node["id"] == demo["start"]
    win.auto_btn.setChecked(True)
    for _ in range(20):
        if not win.auto_btn.isChecked():
            break
        win._auto_step()
    stop = win._current_node()
    assert not win.auto_btn.isChecked() and stop is not None \
        and stop["type"] == "choice", \
        f"自动播放应在 choice 暂停（当前 {stop['id'] if stop else '?'}）"
    print(f"[14] 步进/自动播放 OK（自动暂停于 {stop['id']} choice）")

    # ------------------------------------------------------------------
    # v4：多剧情脚本管理 + 撤销/重做 + 脏标记
    # ------------------------------------------------------------------
    # [15] 多剧情：项目内新建脚本 → 切换 → 节点列表跟随且内容互相隔离
    win.new_story()
    assert len(win._stories) == 1 and win._current_id == "main"
    win._add_story_in_project()  # main 被占 → 自动取 story2
    assert win.story_combo.count() == 2 and win._current_id == "story2"
    assert win.node_list.count() == 1, "新脚本应含 1 个起始节点"
    win._add_node("wait")
    assert win.node_list.count() == 2
    win.story_combo.setCurrentIndex(win.story_combo.findData("main"))
    assert win._current_id == "main" and win.node_list.count() == 1
    win.story_combo.setCurrentIndex(win.story_combo.findData("story2"))
    assert win.node_list.count() == 2
    print("[15] 多剧情脚本管理 OK（新建/切换，列表随切换刷新）")

    # [16] 撤销/重做：跨剧情快照；连续输入合并为一步
    win._undo()  # 撤销 story2 新增的 wait 节点
    assert win._current_id == "story2" and win.node_list.count() == 1
    win._redo()
    assert win.node_list.count() == 2
    win.story_title_edit.setText("改名一")  # 连续两次编辑 → 合并一步
    win.story_title_edit.setText("改名二")
    win._undo()
    assert win.story.get("title") == "新剧情", \
        f"应一步撤销回原标题：{win.story.get('title')!r}"
    win._redo()
    assert win.story.get("title") == "改名二"
    print("[16] 撤销/重做 OK（跨剧情、连续编辑合并为一步）")

    # [17] 脏标记：标题 * + 保存基线 + 撤销/重做联动
    win.new_story()
    assert not win._dirty and "*" not in win.windowTitle()
    win.story_title_edit.setText("改了")
    assert win._dirty and "*" in win.windowTitle()
    win._mark_saved()
    assert not win._dirty
    win._undo()
    assert win._dirty, "撤销回未保存状态应恢复脏标记"
    win._redo()
    assert not win._dirty, "重做回已保存状态应清除脏标记"
    print("[17] 脏标记 OK（标题 *、保存基线、撤销/重做联动）")

    # [18] 脚本 id 改名（表单即改）+ 占用冲突回退 + 删除可撤销
    win._add_story_in_project()
    assert win._current_id == "story2"
    win.story_id_edit.setText("case_a")
    assert win._current_id == "case_a", "改名应同步项目键"
    win.story_id_edit.setText("main")
    assert win._current_id == "case_a" and win.story_id_edit.text() == "case_a", \
        "占用已有 id 应回退并复原文本"
    win._delete_story_in_project()
    assert "case_a" not in win._stories and len(win._stories) == 1
    win._undo()
    assert "case_a" in win._stories, "删除脚本应可撤销"
    print("[18] 脚本改名/占用回退/删除撤销 OK")

    # [19] 未保存确认弹窗（offscreen 模拟点击：放弃修改→继续；取消→中止）
    win._set_dirty(True)
    win._prompt_on_discard = True

    def click_box_button(text: str) -> None:
        # 取当前可见的确认框（旧弹窗关闭后仍是子对象，不可见则跳过）
        boxes = [b for b in win.findChildren(QMessageBox) if b.isVisible()]
        assert boxes, "应弹出确认框"
        box = boxes[-1]
        for b in box.buttons():
            if b.text() == text:
                b.click()
                return
        raise AssertionError(f"确认框缺少按钮 {text!r}")

    QTimer.singleShot(0, lambda: click_box_button("放弃修改"))
    assert win._confirm_discard()
    QTimer.singleShot(0, lambda: click_box_button("取消"))
    assert not win._confirm_discard()
    win._prompt_on_discard = False
    print("[19] 未保存确认弹窗 OK（放弃修改→继续；取消→中止）")

    # [20] ManifestDialog 入口脚本下拉（多剧情回填）
    dlg = main.ManifestDialog("main", editor_data, ["main", "second"],
                              {"entry": "second"})
    assert dlg.manifest()["entry"] == "second", "入口应回填 base manifest"
    dlg2 = main.ManifestDialog("second", editor_data, ["main", "second"])
    assert dlg2.manifest()["entry"] == "second", "入口默认当前脚本"
    print("[20] ManifestDialog 入口下拉 OK")

    win.close()
    app.quit()
    print("SMOKE TEST PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main_fn())
