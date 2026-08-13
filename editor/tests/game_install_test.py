# -*- coding: utf-8 -*-
"""游戏目录自动安装与 Mod 启停管理离线测试。"""

from __future__ import annotations

import json
import struct
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from game_install import (  # noqa: E402
    PREVIEW_REQUEST_NAME,
    GameInstallError,
    GameInstallManager,
)


class GameInstallManagerTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        base = Path(self.temp.name)
        self.game = base / "LegendOfMortal"
        for rel in (
            "Mortal_Data/Managed/.keep",
            "BepInEx/core/BepInEx.Core.dll",
            "BepInEx/core/BepInEx.Unity.Mono.dll",
        ):
            path = self.game / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            if path.suffix:
                path.write_bytes(b"test")
            else:
                path.mkdir(parents=True, exist_ok=True)
        self._write_pe(self.game / "Mortal.exe", 0x014C)
        self.runtime = base / "MortalModHost.dll"
        self.runtime.write_bytes(b"runtime-v1")
        self.settings = base / "settings.json"
        self.manager = GameInstallManager(self.settings, self.runtime)

    @staticmethod
    def _write_pe(path: Path, machine: int) -> None:
        data = bytearray(0x86)
        data[:2] = b"MZ"
        struct.pack_into("<I", data, 0x3C, 0x80)
        data[0x80:0x84] = b"PE\0\0"
        struct.pack_into("<H", data, 0x84, machine)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def tearDown(self):
        self.temp.cleanup()

    def make_mod(self, name="demo.lommod") -> Path:
        path = Path(self.temp.name) / name
        manifest = {
            "format": 1,
            "id": "demo",
            "name": "测试 Mod",
            "version": "1.2.3",
            "author": "测试者",
            "description": "用于自动安装测试",
            "entry": "main",
        }
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("manifest.json", json.dumps(manifest, ensure_ascii=False))
            archive.writestr("lua/main.lua", "return nil")
        return path

    def test_configure_installs_runtime_and_manages_enabled_state(self):
        self.manager.save_game_dir(self.game)
        target, changed = self.manager.install_runtime()
        self.assertTrue(changed)
        self.assertEqual(target.read_bytes(), b"runtime-v1")
        _, changed_again = self.manager.install_runtime()
        self.assertFalse(changed_again)

        installed = self.manager.install_mod(self.make_mod())
        self.assertEqual(installed.parent.name, "mods")
        records = self.manager.list_mods()
        self.assertEqual(len(records), 1)
        self.assertTrue(records[0].enabled)
        self.assertEqual(records[0].name, "测试 Mod")

        disabled = self.manager.set_enabled(installed, False)
        self.assertEqual(disabled.parent.name, "mods_disabled")
        self.assertFalse(self.manager.list_mods()[0].enabled)
        enabled = self.manager.set_enabled(disabled, True)
        self.assertEqual(enabled.parent.name, "mods")

    def test_rejects_wrong_game_dir_and_bad_package(self):
        with self.assertRaises(GameInstallError):
            self.manager.save_game_dir(Path(self.temp.name) / "wrong")
        self.manager.save_game_dir(self.game)
        bad = Path(self.temp.name) / "bad.lommod"
        bad.write_text("not zip", encoding="utf-8")
        with self.assertRaises(GameInstallError):
            self.manager.install_mod(bad)

    def test_bare_game_can_be_configured_then_bepinex_archive_installed(self):
        bare = Path(self.temp.name) / "bare_game"
        self._write_pe(bare / "Mortal.exe", 0x014C)
        managed = bare / "Mortal_Data" / "Managed"
        managed.mkdir(parents=True)
        self.manager.save_game_dir(bare)
        self.assertEqual(self.manager.game_architecture(bare), "x86")
        self.assertFalse(self.manager.bepinex_installed(bare))

        archive_path = Path(self.temp.name) / "bepinex.zip"
        with zipfile.ZipFile(archive_path, "w") as archive:
            archive.writestr("BepInEx/core/BepInEx.Core.dll", b"core")
            archive.writestr("BepInEx/core/BepInEx.Unity.Mono.dll", b"mono")
            archive.writestr("winhttp.dll", b"loader")
            archive.writestr("doorstop_config.ini", b"config")
        self.manager._install_bepinex_archive(archive_path, bare)
        self.assertTrue(self.manager.bepinex_installed(bare))
        self.assertEqual((bare / "winhttp.dll").read_bytes(), b"loader")

    def test_bepinex_archive_rejects_path_traversal(self):
        archive_path = Path(self.temp.name) / "unsafe.zip"
        with zipfile.ZipFile(archive_path, "w") as archive:
            archive.writestr("BepInEx/core/BepInEx.Core.dll", b"core")
            archive.writestr("BepInEx/core/BepInEx.Unity.Mono.dll", b"mono")
            archive.writestr("winhttp.dll", b"loader")
            archive.writestr("../outside.dll", b"bad")
        with self.assertRaises(GameInstallError):
            self.manager._install_bepinex_archive(archive_path, self.game)
        self.assertFalse((Path(self.temp.name) / "outside.dll").exists())

    def test_preview_request_is_atomic_and_validated(self):
        self.manager.save_game_dir(self.game)
        request = self.manager.request_preview("lom_modkit_preview", "main", "n3")
        self.assertEqual(request.name, PREVIEW_REQUEST_NAME)
        data = json.loads(request.read_text(encoding="utf-8"))
        self.assertEqual(
            (data["mod_id"], data["script_id"], data["node_id"]),
            ("lom_modkit_preview", "main", "n3"),
        )
        self.assertFalse(request.with_suffix(".tmp").exists())
        with self.assertRaises(GameInstallError):
            self.manager.request_preview("bad id", "main", "n3")


if __name__ == "__main__":
    unittest.main(verbosity=2)
