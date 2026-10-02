# -*- coding: utf-8 -*-
"""增强版新增功能的测试：批量打开（文件夹 / 多文件）与素材选择器。

用法（在 editor/ 目录下）：
    .venv/Scripts/python tests/plus_open_folder_test.py

覆盖：
1. ``story_json_candidates`` 只认真正的剧情 JSON，跳过 manifest 之类的同级文件；
2. 「打开文件夹」把多个剧情载入为同一项目的多个章节；
3. 章节 id 撞车时自动改名而不是互相覆盖；
4. 「打开多个文件」同样生效；
5. 最近打开 / 自动恢复支持 folder 与 files 两种来源；
6. 素材选择器在无游戏目录（library=None）时仍能打开，并给出明确的缺图说明；
7. 节点表单的人物 / 表情 / 背景字段都带「浏览…」按钮。
"""

from __future__ import annotations

import copy
import json
import os
import sys
import tempfile
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from PySide6.QtWidgets import (  # noqa: E402  # type: ignore[reportMissingImports]
    QApplication,
    QMessageBox,
    QPushButton,
)

import main  # noqa: E402
import models  # noqa: E402
import project_controller  # noqa: E402
from asset_picker_dialog import (  # noqa: E402
    MODE_CHARACTER,
    MODE_PORTRAIT,
    MODE_VIEW,
    AssetPickerDialog,
)
from i18n import t  # noqa: E402


class DialogRecorder:
    """拦下所有模态消息框。

    offscreen 下弹出模态框会一直等用户点确定，把测试进程挂死（在 CI/沙箱里
    会被直接杀掉）。这里换成记账，既能保证测试一定跑完，又能顺手断言
    「确实提示了失败文件」这种行为。
    """

    def __init__(self) -> None:
        self.calls: list[tuple[str, str, str]] = []
        self._saved: dict[str, object] = {}

    def __enter__(self) -> "DialogRecorder":
        for name in ("warning", "critical", "information"):
            self._saved[name] = getattr(QMessageBox, name)
            setattr(QMessageBox, name, self._make(name))
        return self

    def _make(self, kind: str):
        def recorder(_parent=None, _title="", _text="", *_args, **_kwargs):
            self.calls.append((kind, str(_title), str(_text)))
            return QMessageBox.StandardButton.Ok

        return recorder

    def __exit__(self, *_exc) -> None:
        for name, original in self._saved.items():
            setattr(QMessageBox, name, original)

    def texts(self, kind: str = "") -> list[str]:
        return [text for k, _title, text in self.calls if not kind or k == kind]


def _write_story(
    path: Path, story_id: str, title: str, node_text: str, editor_data: dict
) -> None:
    """写一个当前 schema 的合法剧情。

    不能手搓 JSON：``models.load_story`` 会校验 ``story_schema``，缺字段的旧格式
    会被判为需要迁移并直接拒绝。统一走 new_editor_story 才能反映真实读取路径。
    """
    story = main.new_editor_story(story_id, editor_data)
    story["title"] = title
    story["nodes"][1]["text"] = node_text
    path.parent.mkdir(parents=True, exist_ok=True)
    models.save_story(story, path)


def test_candidates_filter(tmp: Path, win) -> None:
    (tmp / "manifest.json").write_text('{"id": "pack"}', encoding="utf-8")
    (tmp / "broken.json").write_text("{ not json", encoding="utf-8")
    (tmp / "notes.txt").write_text("hello", encoding="utf-8")
    _write_story(tmp / "a.json", "a", "甲", "甲台词", win.editor_data)
    sub = tmp / "story"
    sub.mkdir()
    _write_story(sub / "b.json", "b", "乙", "乙台词", win.editor_data)

    found = project_controller.story_json_candidates(tmp)
    names = sorted(path.name for path in found)
    assert names == ["a.json", "b.json"], f"候选筛选结果不对：{names}"
    print("[候选] 只挑出真正的剧情 JSON，跳过 manifest / 坏文件 / 非 JSON")


def test_open_folder(tmp: Path, win) -> None:
    with DialogRecorder() as dialogs:
        original = main.QFileDialog.getExistingDirectory
        try:
            main.QFileDialog.getExistingDirectory = lambda *a, **k: str(tmp)
            win.open_story_folder()
        finally:
            main.QFileDialog.getExistingDirectory = original
        assert dialogs.calls == [], f"合法文件夹不该弹任何框：{dialogs.texts()}"
    assert set(win._stories) == {"a", "b"}, f"应载入 2 个章节，实际 {sorted(win._stories)}"
    assert win.story_combo.count() == 2, "章节下拉应列出 2 个章节"
    assert win._source_kind == "folder"
    assert win._story_paths["a"].name == "a.json"
    assert win._story_paths["b"].name == "b.json"
    print(f"[文件夹] 载入 {len(win._stories)} 个章节，来源 {win._source_kind}")


def test_partial_failure(tmp: Path, win, editor_data: dict) -> None:
    """文件夹里混进旧 schema 的剧情：好的照常载入，坏的明确报出来。"""
    mixed = tmp / "mixed"
    mixed.mkdir()
    _write_story(mixed / "good.json", "good", "好的", "正常台词", editor_data)
    (mixed / "legacy.json").write_text(
        json.dumps({"id": "legacy", "nodes": [{"id": "n1", "type": "end"}]}),
        encoding="utf-8",
    )
    with DialogRecorder() as dialogs:
        original = main.QFileDialog.getExistingDirectory
        try:
            main.QFileDialog.getExistingDirectory = lambda *a, **k: str(mixed)
            win.open_story_folder()
        finally:
            main.QFileDialog.getExistingDirectory = original
    assert set(win._stories) == {"good"}, f"坏的应被跳过：{sorted(win._stories)}"
    warnings = dialogs.texts("warning")
    assert warnings, "有文件读不了时必须提示，不能静默跳过"
    assert "legacy.json" in warnings[0], f"提示里要写明是哪个文件：{warnings[0]}"
    print("[部分失败] 好文件正常载入，坏文件在提示里点名到具体文件名")


def test_id_collision(tmp: Path, win, editor_data: dict) -> None:
    """两个文件写同一个 story id：必须都保留下来并自动改名。"""
    clash = tmp / "clash"
    clash.mkdir()
    _write_story(clash / "one.json", "same", "同 1", "同 1 台词", editor_data)
    _write_story(clash / "two.json", "same", "同 2", "同 2 台词", editor_data)
    with DialogRecorder() as dialogs:
        original = main.QFileDialog.getExistingDirectory
        try:
            main.QFileDialog.getExistingDirectory = lambda *a, **k: str(clash)
            win.open_story_folder()
        finally:
            main.QFileDialog.getExistingDirectory = original
        assert dialogs.calls == [], f"改名不该弹框（只在状态栏说明）：{dialogs.texts()}"
    assert set(win._stories) == {"same", "same_2"}, f"撞 id 未消解：{sorted(win._stories)}"
    texts = {story["nodes"][1]["text"] for story in win._stories.values()}
    assert texts == {"同 1 台词", "同 2 台词"}, "改名过程中剧情内容被覆盖了"
    assert win._dirty, "自动改名后应提示存在未保存改动"
    print("[撞 id] same / same_2 并存，两份内容都在")


def test_open_files(tmp: Path, win) -> None:
    targets = [str(tmp / "a.json"), str(tmp / "story" / "b.json")]
    with DialogRecorder() as dialogs:
        original = main.QFileDialog.getOpenFileNames
        try:
            main.QFileDialog.getOpenFileNames = lambda *a, **k: (targets, "")
            win.open_story_files()
        finally:
            main.QFileDialog.getOpenFileNames = original
        assert dialogs.calls == [], f"多文件载入不该弹框：{dialogs.texts()}"
    assert set(win._stories) == {"a", "b"}, f"多文件载入失败：{sorted(win._stories)}"
    assert win._source_kind == "files"
    print("[多文件] 一次载入 2 个文件")


def test_restore_folder(tmp: Path, win) -> None:
    """重启后应能按「文件夹」来源自动恢复整个项目。"""
    pref_kind = win.game_manager.load_pref("last_open_kind")
    pref_path = win.game_manager.load_pref("last_open_path")
    assert pref_kind == "files", f"最近一次来源应为 files，实际 {pref_kind}"
    assert pref_path, "应记录锚点路径"

    win._stories = {}
    win._current_id = ""
    assert win.restore_last_project(), "自动恢复多文件项目失败"
    assert set(win._stories) == {"a", "b"}, f"恢复后章节不对：{sorted(win._stories)}"
    print("[恢复] 重启后自动恢复 2 个文件组成的项目")


def test_recent_tag(win) -> None:
    win._rebuild_recent_menu()
    texts = [action.text() for action in win._recent_menu.actions()]
    assert any(t("recent.tag_files") in text for text in texts), f"最近列表缺少多文件标记：{texts}"
    print("[最近] 多文件项目在最近列表里有独立标记")


def test_picker_without_library(win) -> None:
    """没有游戏目录时选择器也要能开，并说明为什么没有预览图。"""
    for mode, kwargs in (
        (MODE_CHARACTER, {"initial": "girl4"}),
        (MODE_PORTRAIT, {"portrait_char": "girl4", "initial_emotion": "laugh1"}),
        (MODE_VIEW, {"initial": "center"}),
    ):
        dialog = AssetPickerDialog(
            win, editor_data=win.editor_data, library=None, mode=mode, **kwargs
        )
        assert dialog.char_pane.list.count() > 100, "人物列表没有填充"
        assert dialog.view_pane.list.count() > 100, "背景列表没有填充"
        if mode == MODE_PORTRAIT:
            assert dialog.char_pane.current_value() == "girl4", (
                f"表情模式应锁定到指定人物，实际 {dialog.char_pane.current_value()}"
            )
            assert dialog.char_pane.emotion() == "laugh1"
        dialog.status.setText("")
        dialog.refresh_preview(dialog._active_pane())
        assert dialog.status.text() == t("picker.no_library"), (
            f"[{mode}] 缺图时应说明原因，实际：{dialog.status.text()!r}"
        )
        dialog.close()
    print("[选择器] 无游戏目录时三种模式都能打开，并给出明确缺图说明")


def test_browse_buttons(win) -> None:
    """人物 / 表情 / 背景字段都要有「浏览…」按钮。"""
    cases = [
        ({"id": "n", "type": "say", "text": "x", "character": "girl4", "portrait": "normal"}),
        ({"id": "n", "type": "scene", "view": "center"}),
    ]
    for node in cases:
        win.form.set_node(copy.deepcopy(node))
        labels = [b.text() for b in win.form.findChildren(QPushButton)]
        assert t("picker.browse") in labels, f"{node['type']} 字段缺少浏览按钮：{labels}"
    print("[表单] 人物 / 表情 / 背景字段都带「浏览…」按钮")


def main_fn() -> int:
    app = QApplication([])
    editor_data, is_fallback = models.load_editor_data(main.PROJECT_ROOT)
    win = main.MainWindow(editor_data, is_fallback)
    win._prompt_on_discard = False
    win.show()
    app.processEvents()

    with tempfile.TemporaryDirectory(prefix="lom_plus_open_") as raw:
        tmp = Path(raw)
        # offscreen 下 _should_persist_session() 默认返回 False（免得测试污染真实
        # 配置），这里换成临时 settings 文件并强制开启，才能验证「最近打开 / 恢复」。
        win.game_manager.settings_path = tmp / "settings.json"
        win._should_persist_session = lambda: True

        test_candidates_filter(tmp, win)
        test_open_folder(tmp, win)
        test_partial_failure(tmp, win, editor_data)
        test_id_collision(tmp, win, editor_data)
        test_open_files(tmp, win)
        test_restore_folder(tmp, win)
        test_recent_tag(win)
        test_picker_without_library(win)
        test_browse_buttons(win)

    win.close()
    print("\nplus_open_folder_test 全部通过")
    return 0


if __name__ == "__main__":
    sys.exit(main_fn())
