"""Integration: python python/tests/test_authoring.py PATH/TO/lomc.exe (or lom-editor.exe)."""
import importlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

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
