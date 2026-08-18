# -*- coding: utf-8 -*-
from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
import zipfile

EDITOR = Path(__file__).resolve().parents[1]
if str(EDITOR) not in sys.path:
    sys.path.insert(0, str(EDITOR))

from diagnostic_bundle import (  # noqa: E402
    MAX_LOG_OUTPUT_CHARS,
    detect_game_version,
    detect_runtime_version,
    export_diagnostic_bundle,
    sanitize_text,
)
from app_version import RUNTIME_VERSION  # noqa: E402
from game_install import GameInstallManager  # noqa: E402


class DiagnosticBundleTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.game = self.base / "steamapps" / "common" / "LegendOfMortal"
        self.game.mkdir(parents=True)
        self.settings = self.base / "settings.json"
        self.settings.write_text(
            json.dumps({"game_dir": str(self.game)}), encoding="utf-8"
        )
        self.bundled_runtime = self.base / "bundled" / "MortalModHost.dll"
        self.bundled_runtime.parent.mkdir()
        self.bundled_runtime.write_bytes(b"runtime-v0.6.0")
        installed = self.game / "BepInEx" / "plugins" / "MortalModHost" / "MortalModHost.dll"
        installed.parent.mkdir(parents=True)
        installed.write_bytes(self.bundled_runtime.read_bytes())
        self.manager = GameInstallManager(
            settings_path=self.settings,
            runtime_dll=self.bundled_runtime,
            doorstop_dll=self.base / "unused.dll",
        )

    def tearDown(self):
        self.temp.cleanup()

    def test_fixed_allowlist_versions_redaction_and_bounded_logs(self):
        appmanifest = self.base / "steamapps" / "appmanifest_1859910.acf"
        appmanifest.write_text(
            '"AppState"\n{\n  "appid" "1859910"\n  "buildid" "20337760"\n}\n',
            encoding="utf-8",
        )
        runtime_log = self.game / "BepInEx" / "LogOutput.log"
        runtime_log.parent.mkdir(parents=True, exist_ok=True)
        runtime_log.write_text(
            "[Info:MortalModHost] MortalModHost 0.6.0 启动，扫描 mods 目录：%s\n"
            "[Info:OtherPlugin] UNRELATED_PRIVATE_PAYLOAD\n"
            "[Error:MortalModHost] [mod-runtime-error] C:\\Users\\Alice\\secret\\trace.txt\n"
            "%s\n"
            % (self.game / "BepInEx" / "plugins", "MortalModHost relevant " + "x" * (MAX_LOG_OUTPUT_CHARS + 1000)),
            encoding="utf-8",
        )
        crash_log = self.base / "private" / "crash.log"
        crash_log.parent.mkdir()
        crash_log.write_text(
            "editor crash at %s\n" % (self.base / "private" / "source.py"),
            encoding="utf-8",
        )
        manifest = {
            "format": 1,
            "id": "diagnostic_test",
            "name": "Diagnostic Test",
            "version": "1.2.3",
            "entry": "main",
            "description": "asset at " + str(self.game / "private" / "plot.txt"),
        }
        stories = {
            "main": {
                "id": "main",
                "title": "PRIVATE_STORY_TITLE",
                "start": "n1",
                "nodes": [
                    {"id": "n1", "type": "say", "mode": "narrative", "text": "DIALOGUE_SECRET"},
                    {"id": "n2", "type": "background", "image": "user:test.image"},
                    {"id": "n3", "type": "raw", "lua": "RAW_LUA_SECRET"},
                    {"id": "n4", "type": "end"},
                ],
            }
        }
        issues = [SimpleNamespace(
            severity="error", code="missing_image", story_id="main", node_id="n2",
            message="missing at " + str(self.game / "assets" / "secret.png"), fixable=False,
        )]
        output = export_diagnostic_bundle(
            self.base / "result.diagnostics",
            stories,
            {"schema": 3, "characters": [{"name": "PRIVATE_CHARACTER"}]},
            manifest,
            issues,
            game_manager=self.manager,
            crash_log=crash_log,
        )
        self.assertEqual(output.suffix, ".zip")
        self.assertFalse(any(path.suffix == ".tmp" for path in self.base.iterdir()))
        with zipfile.ZipFile(output) as archive:
            self.assertEqual(set(archive.namelist()), {
                "diagnostic.json", "validation.json", "logs/editor-crash.log",
                "logs/runtime.log", "README.txt",
            })
            diagnostic = json.loads(archive.read("diagnostic.json"))
            validation = json.loads(archive.read("validation.json"))
            runtime_text = archive.read("logs/runtime.log").decode("utf-8")
            all_text = "\n".join(
                archive.read(name).decode("utf-8", errors="replace")
                for name in archive.namelist()
            )
        self.assertEqual(diagnostic["editor_version"], "1.1.0")
        self.assertEqual(diagnostic["runtime_version"], "1.1.0")
        self.assertEqual(diagnostic["detected_game_version"], "Steam build 20337760")
        self.assertEqual(diagnostic["manifest"]["id"], "diagnostic_test")
        self.assertEqual(diagnostic["project_metadata"]["story_count"], 1)
        self.assertEqual(diagnostic["project_metadata"]["node_count"], 4)
        self.assertEqual(diagnostic["project_metadata"]["raw_node_count"], 1)
        self.assertEqual(validation["errors"], 1)
        self.assertIn("<game-dir>", validation["issues"][0]["message"])
        self.assertNotIn("UNRELATED_PRIVATE_PAYLOAD", runtime_text)
        self.assertLessEqual(len(runtime_text), MAX_LOG_OUTPUT_CHARS)
        self.assertNotIn(str(self.base), all_text)
        self.assertNotIn("C:\\Users\\Alice", all_text)
        self.assertNotIn("DIALOGUE_SECRET", all_text)
        self.assertNotIn("RAW_LUA_SECRET", all_text)
        self.assertNotIn("PRIVATE_STORY_TITLE", all_text)
        self.assertNotIn("PRIVATE_CHARACTER", all_text)

    def test_version_fallbacks_and_path_sanitizer(self):
        installed = self.game / "BepInEx" / "plugins" / "MortalModHost" / "MortalModHost.dll"
        installed.write_bytes(b"different runtime")
        self.assertEqual(
            detect_runtime_version(
                self.manager, self.game,
                "[Info:MortalModHost] MortalModHost 9.8.7 启动，扫描 mods 目录：x",
            ),
            "9.8.7",
        )
        self.assertEqual(detect_game_version(None), "not configured")
        sanitized = sanitize_text(
            "C:\\Users\\Alice\\secret.txt /home/alice/private.txt " + str(self.game),
            self.game,
        )
        self.assertNotIn("Alice", sanitized)
        self.assertNotIn("/home/alice", sanitized)
        self.assertNotIn(str(self.game), sanitized)

    def test_editor_runtime_version_constant_matches_host(self):
        plugin_source = EDITOR.parent / "runtime" / "MortalModHost" / "src" / "Plugin.cs"
        text = plugin_source.read_text(encoding="utf-8")
        self.assertIn('public const string VERSION = "%s"' % RUNTIME_VERSION, text)


if __name__ == "__main__":
    unittest.main()
