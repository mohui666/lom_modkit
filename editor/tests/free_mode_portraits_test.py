import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
ROOT = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT / "editor"), str(ROOT / "compiler")]
from PySide6.QtWidgets import QApplication, QComboBox
from PySide6.QtGui import QPixmap, QColor
from lomc import compile_story, validate_story
from lomc.pack import pack_mod
from lomc.errors import LomcError
import models
from project_templates import create_project_template
from preview import simulate_stage, build_playtest_prelude
from preview_library import read_preview_library
from character_preview import CharacterPortraitPreview
from node_form import NodeForm
from schema_versions import manifest_versions


class FreeModePortraitsTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])

    def write_project(self, root, project):
        manifest = {**manifest_versions(), **project["manifest"], "id": "free_demo", "name": "自由模式", "version": "1.0", "author": "test", "description": "test", "entry": "main"}
        (root / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        (root / "story").mkdir()
        for sid, story in project["stories"].items():
            (root / "story" / (sid+".json")).write_text(json.dumps(story), encoding="utf-8")

    def test_free_mode_package_has_time_place_once_and_returns_to_free(self):
        project = create_project_template("free_mode_story")
        trigger = project["manifest"]["campaign"]["triggers"][0]
        self.assertEqual((trigger["position"], trigger["when_month"], trigger["when_stage"]), ("Center", 4, 1))
        self.assertIn("四月上旬", project["stories"]["main"]["nodes"][0]["text"])
        event = project["stories"][trigger["script"]]
        self.assertEqual(event["nodes"][-2]["flag"], trigger["when_flag_clear"])
        self.assertFalse(project["manifest"]["campaign"].get("disable_official_events", False))
        with tempfile.TemporaryDirectory() as td:
            root = Path(td); self.write_project(root, project)
            package = pack_mod(str(root), str(root / "demo.lommod"))
            with zipfile.ZipFile(package) as archive:
                for sid in project["stories"]:
                    self.assertIn('luamanager.ChangeScene("Free", "", "")', archive.read("lua/"+sid+".lua").decode())
                self.assertEqual(json.loads(archive.read("manifest.json"))["campaign"]["triggers"], [trigger])

    def test_beautified_picker_writes_real_player_and_compiles_host_bridge(self):
        form = NodeForm(); form.set_context(models.FALLBACK_EDITOR_DATA, ["show1"])
        node = {"id":"show1", "type":"show", "character":"player", "position":"M", "portrait":"normal"}
        form.set_node(node)
        combo = next(c for c in form.findChildren(QComboBox) if c.findData("player_beautified") >= 0)
        combo.setCurrentIndex(combo.findData("player_beautified"))
        self.assertEqual((node["character"],node["appearance"]), ("player","beautified"))
        story = {"id":"main","start":"show1","nodes":[node,{"id":"end1","type":"end"}]}
        lua = compile_story(story)
        self.assertLess(lua.index('mod_player_appearance("beautified")'), lua.index('characters.LoadCharacterAsset("player")'))
        with self.assertRaises(LomcError):
            validate_story({**story,"nodes":[{**node,"character":"brother4"},story["nodes"][-1]]})

    def test_appearance_survives_stage_simulation_and_f5_prelude(self):
        story = {"id":"main","start":"show1","nodes":[
            {"id":"show1","type":"show","character":"player","position":"M","appearance":"beautified"},
            {"id":"say1","type":"say","character":"player","text":"test"},
            {"id":"end1","type":"end"}]}
        self.assertEqual(simulate_stage(story,"say1")["actors"]["player"]["appearance"],"beautified")
        self.assertEqual(next(n for n in build_playtest_prelude(story,"say1") if n["type"]=="show")["appearance"], "beautified")

    def test_appearance_export_requires_compatible_host(self):
        project = create_project_template("empty")
        project["stories"]["main"]["nodes"].insert(0,{"id":"show1","type":"show","character":"player","position":"M","appearance":"beautified"})
        project["stories"]["main"]["start"]="show1"
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);self.write_project(root,project)
            package=pack_mod(str(root),str(root/"demo.lommod"))
            with zipfile.ZipFile(package) as archive:
                self.assertEqual(json.loads(archive.read("manifest.json"))["min_host_version"],"1.1.2")
            manifest=json.loads((root/"manifest.json").read_text());manifest["min_host_version"]="1.1.1"
            (root/"manifest.json").write_text(json.dumps(manifest))
            with self.assertRaisesRegex(LomcError,"min_host_version"):
                pack_mod(str(root),str(root/"bad.lommod"))
            manifest["min_host_version"] = "1.1.2-beta.1"
            (root/"manifest.json").write_text(json.dumps(manifest))
            with self.assertRaisesRegex(LomcError, "min_host_version"):
                pack_mod(str(root), str(root/"prerelease.lommod"))
            manifest["min_host_version"] = "1.1.2+local"
            (root/"manifest.json").write_text(json.dumps(manifest))
            pack_mod(str(root), str(root/"metadata.lommod"))

    def test_preview_uses_selected_portrait_pixels_and_rejects_external_paths(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td); image=QPixmap(100,160); image.fill(QColor("#00ff00"));image.save(str(root/"normal.png"))
            mapping={"characters":{"player":{"portraits":{"normal":"normal.png"}}},"views":{}}
            (root/"preview_map.json").write_text(json.dumps(mapping))
            data,directory=read_preview_library(root)
            widget=CharacterPortraitPreview();widget.set_assets(data,directory);widget.resize(400,500);widget.set_character("player")
            pixel=widget.grab().toImage().pixelColor(200,250)
            self.assertEqual(pixel,QColor("#00ff00"))
            mapping["characters"]["player"]["portraits"]["normal"]="../outside.png"
            (root/"preview_map.json").write_text(json.dumps(mapping))
            with self.assertRaises(ValueError): read_preview_library(root)


if __name__ == "__main__": unittest.main()
