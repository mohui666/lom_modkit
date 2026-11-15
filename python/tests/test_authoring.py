"""Integration: python python/tests/test_authoring.py PATH/TO/lomc.exe (or lom-editor.exe)."""
import importlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

EXECUTABLE = Path(sys.argv.pop(1)).resolve()
ROOT = Path(__file__).resolve().parents[2]


class AuthoringTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix="lom-python-api-")
        cls.sdk = Path(cls.temp.name) / "sdk"
        cls.command = [str(EXECUTABLE)] + (["--cli"] if EXECUTABLE.stem == "lom-editor" else [])
        subprocess.run([*cls.command, "sdk", "-o", str(cls.sdk), "--json"], check=True, capture_output=True)
        sys.path.insert(0, str(cls.sdk))
        cls.api = importlib.import_module("lom_modkit")
        cls.cli = cls.api.CLI(EXECUTABLE)

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_bad_constructor_parameters_fail_before_build(self):
        a = self.api
        for constructor, kwargs in [
            (a.Say, {"text": "hello", "typo": 1}),
            (a.Say, {}),
            (a.Say, {"text": "hello", "mode": "wrong"}),
            (a.Wait, {"seconds": True}),
            (a.Wait, {"seconds": float("nan")}),
            (a.Show, {"character": "player", "position": "M", "fadeDuration": "1"}),
        ]:
            with self.subTest(constructor=constructor.__name__, kwargs=kwargs):
                with self.assertRaises((TypeError, ValueError)):
                    constructor(**kwargs)
        with self.assertRaises(ValueError):
            a.Chapter(id="main", title="重复", nodes=[a.End(id="s"), a.End(id="s")])
        with self.assertRaises(ValueError):
            a.Chapter(id="main", title="无入口", nodes=[a.End(id="s")], start="missing")
        self.assertEqual(a.Move(character="player", from_="M", to="R").to_dict()["from"], "M")

    def test_every_compiler_node_has_a_constructor(self):
        schema = self.cli.run("schema", "nodes")
        self.assertEqual(set(schema["nodes"]), set(self.api.NODE_TYPES))
        kinds = set()
        for file in (ROOT / "samples/showcase3/story").glob("*.json"):
            story = json.loads(file.read_text(encoding="utf-8"))
            for node in story["nodes"]:
                self.assertEqual(self.api.Node.from_dict(node).to_dict(), node)
                kinds.add(node["type"])
        self.assertEqual(set(schema["nodes"]), kinds)

    def test_optional_staging_returns_existing_nodes_and_restores_explicit_placement(self):
        a = self.api
        ids = a.NodeIds(["show1", {"id": "show3"}])
        stage = a.Staging(ids)
        enter = stage.enter("player", "L", portrait="normal", facing="right", scale=90, wait_after=0.1)
        self.assertEqual(enter.id, "show2")
        enter.fields["position"] = "R"
        restored = stage.restore("player", moveDuration=0.2)
        self.assertEqual(restored.id, "show4")
        self.assertEqual(restored.fields["position"], "L")
        self.assertEqual(restored.fields["scale"], 90)
        self.assertNotIn("wait_after", restored.fields)
        together = stage.together(a.Shock(character="player", duration=0.1),
                                  a.Mood(enabled=True, character="player", icon="shock"))
        self.assertTrue(all("id" not in action for action in together.fields["actions"]))
        self.assertNotIn("wait_for_completion", a.Show(character="player", position="M").fields)
        for invalid in (
            lambda: stage.restore("missing"),
            lambda: stage.together(a.Say(text="不是并行动作"), a.Shock(character="player")),
            lambda: stage.together(a.Show(character="player", position="M", goto="end"), a.Shock(character="player")),
            lambda: stage.enter("player", "M", typo=True),
        ):
            with self.assertRaises((ValueError, TypeError)):
                invalid()
        chapter = a.Chapter(id="main", title="演出助手", nodes=[
            enter, restored, together, stage.exit("player"), a.End(id=ids("end"))])
        output = Path(self.temp.name) / "staging"
        self.cli.build(a.Project(id="sdk_staging", name="演出助手", chapters=[chapter]), output)
        package = Path(self.temp.name) / "staging.lommod"
        self.cli.export(output, package)
        reopened = self.cli.run("project-read", package)["project"]
        self.assertEqual(reopened["stories"]["main"]["nodes"], chapter.to_dict()["nodes"])

    def test_preview_asset_and_status_conveniences_forward_without_polling(self):
        with patch.object(self.cli, "operation", return_value={"ok": True}) as operation:
            self.cli.render_preview("work", "main", "s", "frame.png", width=720)
            operation.assert_called_with("render-preview", project="work", script="main", node="s", output="frame.png", width=720)
            self.cli.render_previews("work", [{"script": "main", "node": "s"}], "frames", height=720)
            operation.assert_called_with("render-previews", project="work", frames=[{"script": "main", "node": "s"}], output="frames", height=720)
            self.cli.render_video("work", "game", "story.mp4", scope="project", choices={"main/route": 1}, background=False)
            operation.assert_called_with("render-video", project="work", game="game", output="story.mp4", scope="project", choices={"main/route": 1}, background=False)
            self.cli.playtest_status("game", "request")
            operation.assert_called_with("playtest-status", game="game", request_id="request")
            self.cli.prepare_event_playtest("work", "visit", "event.lommod", entry_mode="predecessor", source_script="prologue")
            operation.assert_called_with("prepare-event-playtest", project="work", event_id="visit", output="event.lommod", entry_mode="predecessor", source_script="prologue")
            self.cli.playtest_event("work", "visit", "game", entry_mode="boundary", year=1, month=2, stage=3)
            operation.assert_called_with("playtest-event", project="work", event_id="visit", game="game", entry_mode="boundary", year=1, month=2, stage=3)
            self.cli.query_assets(kind="character", query="师兄")
            operation.assert_called_with("asset-query", kind="character", query="师兄")
        def read_request(command, path):
            request = json.loads(Path(path).read_text(encoding="utf-8"))
            for field in ("library", "cache_dir", "game"):
                self.assertTrue(Path(request[field]).is_absolute())
            return {"ok": True}
        with patch.object(self.cli, "run", side_effect=read_request):
            self.cli.query_assets(library="library", cache_dir="cache", game="game")

    def test_document_constructors_and_inline_assets(self):
        a = self.api
        with tempfile.TemporaryDirectory(prefix="lom-typed-documents-") as directory:
            root = Path(directory)
            png = next((ROOT / "samples/showcase3/assets").rglob("*.png"))
            art = a.ImageAsset(id="typed.picture", name="测试插图", main=png)
            chapter = a.Chapter(id="main", title="类型化配置", nodes=[
                a.Overlay(id="image", action="show", slot="picture", image=art.ref),
                a.Say(id="s", mode="narrative", text="开场"), a.End(id="e")],
                localization=a.Localization(translations=[a.Translation(locale="cht", texts={"s.text": "開場"})]))
            task = a.Task(id="visit", name="拜访", states=["active", "done"], initial="active")
            scenario = a.Scenario(entry_event="opening", tasks=[task], events=[
                a.Event(id="opening", name="开场", script="main", trigger="start",
                    outcomes={"completed": a.Outcome(tasks={"visit": "done"}, variables={"visits": a.VariableChange(op="add", value=1)})})])
            project = a.Project(id="typed_project", name="类型化工程", mode="sandbox", chapters=[chapter], scenario=scenario, assets=[art])
            self.cli.build(project, root / "project")
            self.cli.export(root / "project", root / "work.lommod")
            read = self.cli.run("project-read", root / "work.lommod")["project"]
            self.assertEqual(read["scenario"], scenario.to_dict())
            self.assertEqual(read["stories"]["main"]["localization"]["translations"]["cht"]["s.text"], "開場")
            with self.assertRaises(ValueError):
                a.Scenario(tasks=[task], events=[a.Event(id="x", name="x", script="main", trigger="auto", outcomes={"completed": a.Outcome(tasks={"visit": "missing"})})])
            with self.assertRaises(ValueError):
                a.VariableChange(op="add", value=True)
            with self.assertRaises(TypeError):
                a.ImageAsset(id="typed.picture", name="bad", main=png, typo=1)

    def test_story_entry_event_fields_follow_the_existing_original_chapter_contract(self):
        a = self.api
        params = dict(id="visit", name="入口", script="main", trigger="story_entry")
        event = a.Event(**params, target="chapter-1", from_="prologue_2")
        data = event.to_dict()
        self.assertEqual((data["target"], data["from"]), ("chapter-1", "prologue_2"))
        self.assertNotIn("from_", data)
        self.assertEqual(a.Event(**data).to_dict(), data)
        self.assertNotIn("from", a.Event(**params, target="a" * 64).to_dict())
        with self.assertRaises(TypeError):
            a.Event(**params, target="chapter", from_="one", **{"from": "two"})
        for field in ("target", "from_"):
            for invalid in ("", "../chapter", "MOD_other_main", "chapter.lua", "x" * 65, None, 7):
                kwargs = {"target": "chapter", "from_": "prologue", field: invalid}
                with self.subTest(field=field, value=invalid), self.assertRaises(ValueError):
                    a.Event(**params, **kwargs)
        with self.assertRaises(ValueError):
            a.Event(**params)
        with self.assertRaises(ValueError):
            a.Event(**params, target="chapter", position="Center")
        for trigger in ("start", "auto", "position"):
            legacy_params = {**params, "trigger": trigger}
            if trigger == "position":
                legacy_params["position"] = "Center"
            legacy = a.Event(**legacy_params).to_dict()
            self.assertNotIn("target", legacy)
            self.assertNotIn("from", legacy)
            self.assertEqual(a.Event(**legacy).to_dict(), legacy)
            for entry_fields in ({"target": "chapter"}, {"from_": "prologue"}, {"target": None}, {"from": None}):
                with self.subTest(trigger=trigger, fields=entry_fields), self.assertRaises(ValueError):
                    a.Event(**legacy_params, **entry_fields)

    def test_story_entry_event_survives_build_export_edit_and_reopen(self):
        a = self.api
        root = Path(self.temp.name) / "story-entry-event"
        event = a.Event(id="visit", name="章节入口", script="main", trigger="story_entry",
                        target="section_01_free_03", from_="section_01_free_start_02")
        scenario = a.Scenario(events=[event])
        chapter = a.Chapter(id="main", title="入口回归", nodes=[
            a.Say(id="s", mode="narrative", text="进入章节后演出。"), a.End(id="e")])
        project = a.Project(id="sdk_story_entry", name="SDK 章节入口", mode="addon",
                            chapters=[chapter], scenario=scenario)
        self.cli.build(project, root / "project")
        package = root / "work.lommod"
        self.cli.export(root / "project", package)
        original = package.read_bytes()
        read = self.cli.run("project-read", package)["project"]
        self.assertEqual(read["scenario"], scenario.to_dict())
        editable = a.Project.open(self.cli, package)
        identity = editable.manifest["campaign_id"]
        editable.stories["main"]["nodes"][0]["text"] = "保留入口条件的修改。"
        self.cli.build(editable, root / "edited")
        self.cli.export(root / "edited", root / "edited.lommod")
        reopened = self.cli.run("project-read", root / "edited.lommod")["project"]
        self.assertEqual(reopened["scenario"], scenario.to_dict())
        self.assertEqual(reopened["manifest"]["campaign_id"], identity)
        self.assertEqual(reopened["stories"]["main"]["nodes"][0]["text"], "保留入口条件的修改。")
        self.assertEqual(package.read_bytes(), original)

    def test_python_example_generates_editable_project_and_package(self):
        output = Path(self.temp.name) / "example"
        env = {**os.environ, "PYTHONIOENCODING": "utf-8"}
        result = subprocess.run([sys.executable, str(self.sdk / "example.py"), str(EXECUTABLE), str(output)],
                                capture_output=True, encoding="utf-8", env=env)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        project = self.cli.run("project-read", output)["project"]
        self.assertEqual(project["stories"]["main"]["nodes"][0]["text"], "山雨初歇，远处传来钟声。")
        package = output.parent / "山门初见.lommod"
        self.cli.run("inspect", package)
        old = self.api.Project.open(self.cli, package)
        identity = old.manifest["campaign_id"]
        old.stories["main"]["nodes"][0]["text"] = "新的开场。"
        copied = output.parent / "copy"
        self.cli.build(old, copied)
        reopened = self.cli.run("project-read", copied)["project"]
        self.assertEqual(reopened["manifest"]["campaign_id"], identity)
        self.assertEqual(reopened["stories"]["main"]["nodes"][0]["text"], "新的开场。")

    def test_scenario_translation_assets_and_parallel(self):
        a = self.api
        with tempfile.TemporaryDirectory(prefix="lom-author-content-") as directory:
            root = Path(directory)
            png = next((ROOT / "samples/showcase3/assets").rglob("*.png"))
            metadata = root / "content.json"
            metadata.write_text(json.dumps({"schema": 1, "content_schema": 1, "id": "demo.bg",
                                           "name": "测试插图", "type": "image", "files": {"main": "bg.png"}}), encoding="utf-8")
            chapter = a.Chapter(id="main", title="事件", nodes=[
                a.Show(id="enter", character="player", position="M"),
                a.Parallel(id="p", actions=[a.Shock(character="player", duration=0.1).action(),
                                           a.Mood(enabled=True, character="player", icon="shock").action()]),
                a.Say(id="s", mode="narrative", text="事件开场。"),
                a.Overlay(id="picture", action="show", slot="test", image="user:demo.bg"),
                a.End(id="e"),
            ], localization={"default_locale": "chs", "translations": {"cht": {"s.text": "事件開場。"}}})
            project = a.Project(id="sdk_event", name="事件测试", mode="addon", chapters=[chapter],
                                assets={"assets/user/image/demo.bg/content.json": metadata,
                                        "assets/user/image/demo.bg/bg.png": png})
            project.recipe["scenario"] = {
                "schema": 1, "tasks": [{"id": "visit", "name": "到访", "initial": "active", "states": ["active", "done"]}],
                "events": [{"id": "opening", "name": "开场", "trigger": "position", "position": "Center",
                            "script": "main", "once": True, "when": {"all": []},
                            "outcomes": {"completed": {"variables": {}, "tasks": {"visit": "done"}}}}],
            }
            self.cli.build(project, root / "project")
            self.cli.export(root / "project", root / "work.lommod")
            read = self.cli.run("project-read", root / "work.lommod")
            self.assertIn("assets/user/image/demo.bg/bg.png", read["assets"])
            self.assertEqual(read["project"]["scenario"]["events"][0]["script"], "main")
            self.assertEqual(read["project"]["scenario"]["tasks"][0]["id"], "visit")
            self.assertEqual(self.cli.run("texts", root / "project/story/main.json")["texts"]["s.text"], "事件开场。")
            self.cli.run("compile", root / "project/story/main.json", "--locale", "cht", "-o", root / "main.lua")
            self.assertIn("s.text", read["project"]["stories"]["main"]["localization"]["translations"]["cht"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
