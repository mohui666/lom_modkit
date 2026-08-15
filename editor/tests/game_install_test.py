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
from unittest import mock

EDITOR_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(EDITOR_DIR))

from game_install import (  # noqa: E402
    PREVIEW_REQUEST_NAME,
    GameInstallError,
    GameInstallManager,
    _choose_zombie_prefix,
    _ensure_ignore_disable_switch,
    _is_mod_read_story_key,
    build_story_read_keys,
    reset_story_read_state,
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
        self.doorstop = base / "win-x86-doorstop.dll"
        self.doorstop.write_bytes(b"patched-doorstop")
        self.settings = base / "settings.json"
        self.manager = GameInstallManager(self.settings, self.runtime, self.doorstop)

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

    def test_manifest_reader_rejects_runtime_bypasses_and_oversize(self):
        def write_manifest(filename: str, manifest: dict | None = None, raw: bytes | None = None) -> Path:
            path = Path(self.temp.name) / filename
            payload = raw if raw is not None else json.dumps(manifest, ensure_ascii=False).encode("utf-8")
            with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
                archive.writestr("manifest.json", payload)
                archive.writestr("lua/main.lua", "return nil")
            return path

        base = {
            "format": 1,
            "id": "safe_mod",
            "name": "测试",
            "version": "1",
            "author": "作者",
            "description": "简介",
            "entry": "main",
        }
        bad_id = dict(base, id="official/../fake")
        bad_entry = dict(base, entry="../main")
        bad_name = dict(base, name="伪装\u202e官方")
        for filename, manifest in (
            ("bad_id.lommod", bad_id),
            ("bad_entry.lommod", bad_entry),
            ("bad_name.lommod", bad_name),
        ):
            with self.subTest(filename=filename):
                with self.assertRaises(GameInstallError):
                    self.manager._read_manifest(write_manifest(filename, manifest))

        oversized = b"{" + b" " * (4 * 1024 * 1024) + b"}"
        with self.assertRaises(GameInstallError):
            self.manager._read_manifest(write_manifest("huge_manifest.lommod", raw=oversized))

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

    def test_ensure_ignore_disable_switch(self):
        text, changed = _ensure_ignore_disable_switch(
            "enabled = true\nignore_disable_switch = false\n"
        )
        self.assertTrue(changed)
        self.assertIn("ignore_disable_switch = true", text)
        again, changed_again = _ensure_ignore_disable_switch(text)
        self.assertFalse(changed_again)
        self.assertEqual(again, text)
        appended, added = _ensure_ignore_disable_switch("[General]\nenabled = true\n")
        self.assertTrue(added)
        self.assertTrue(appended.endswith("ignore_disable_switch = true\n"))

    def test_apply_steam_launch_fix_rewrites_proxy_and_ini(self):
        self.manager.save_game_dir(self.game)
        (self.game / "winhttp.dll").write_bytes(b"official-proxy")
        (self.game / "doorstop_config.ini").write_text(
            "[General]\nenabled = true\nignore_disable_switch = false\n",
            encoding="utf-8",
        )
        actions = self.manager.apply_steam_launch_fix()
        self.assertTrue(any("ignore_disable_switch" in item for item in actions))
        self.assertTrue((self.game / "version.dll").is_file())
        self.assertEqual((self.game / "version.dll").read_bytes(), b"patched-doorstop")
        self.assertFalse((self.game / "winhttp.dll").is_file())
        self.assertTrue((self.game / "winhttp.dll.lom_bak").is_file())
        self.assertTrue(self.manager.steam_launch_fix_applied())
        second = self.manager.apply_steam_launch_fix()
        self.assertEqual(second, ["Steam 启动修复已经就绪，无需再改。"])

    def test_apply_steam_launch_fix_requires_bepinex(self):
        bare = Path(self.temp.name) / "bare_no_bep"
        self._write_pe(bare / "Mortal.exe", 0x014C)
        (bare / "Mortal_Data" / "Managed").mkdir(parents=True)
        self.manager.save_game_dir(bare)
        with self.assertRaises(GameInstallError):
            self.manager.apply_steam_launch_fix()


class ResetStoryReadStateTest(unittest.TestCase):
    """reset_story_read_state：仅用临时文件，不碰真实 LocalLow 存档。"""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.slot = Path(self.temp.name) / "76561198000000000"
        self.slot.mkdir(parents=True)
        self.dat = self.slot / "Save_universe.dat"
        self.json_path = self.slot / "Save_universe.json"
        self.mod_id = "showcase2"

    def tearDown(self):
        self.temp.cleanup()

    def _write_dat(self, *keys: str) -> None:
        # 模拟 BinaryFormatter 里夹杂的 UTF-8 key 字节（等长替换只依赖子串）
        blob = b"\x00HEADER\x00" + b"\x00".join(k.encode("utf-8") for k in keys) + b"\x00TAIL"
        self.dat.write_bytes(blob)

    def _write_json(self, keys: list[str], **extra) -> None:
        payload = {"Version": "1", "ReadStoryData": keys, **extra}
        self.json_path.write_text(json.dumps(payload, ensure_ascii=False), encoding="utf-8")

    def _keys(self, mod_id: str | None = None) -> list[str]:
        mid = mod_id or self.mod_id
        return [
            f"MOD_{mid}_main_n1",
            f"MOD_{mid}_main_n2",
            f"MOD_{mid}_main_n3",
            f"MOD_{mid}_second_a",
        ]

    def _reset(self, *, extra_ids: list[str] | None = None):
        ids = extra_ids or []
        key_map = {self.mod_id: self._keys()}
        key_map.update({mid: self._keys(mid) for mid in ids})
        return reset_story_read_state(
            self.mod_id,
            saves=[self.dat],
            extra_ids=ids,
            read_keys_by_id=key_map,
        )

    def test_rejects_invalid_mod_id(self):
        with self.assertRaises(GameInstallError):
            reset_story_read_state("Bad Id", saves=[self.dat])

    def test_dat_equal_length_rename_and_backup(self):
        live = [
            f"MOD_{self.mod_id}_main_n1",
            f"MOD_{self.mod_id}_main_n2",
            "vanilla_key",
        ]
        self._write_dat(*live)
        results = self._reset()
        self.assertEqual(results, [(self.dat, 2)])
        raw = self.dat.read_bytes()
        self.assertNotIn(f"MOD_{self.mod_id}_".encode("utf-8"), raw)
        self.assertIn(f"mod_{self.mod_id}_".encode("utf-8"), raw)
        self.assertIn(b"vanilla_key", raw)
        bak = self.dat.with_suffix(self.dat.suffix + ".lomkit_bak")
        self.assertTrue(bak.is_file())
        self.assertIn(f"MOD_{self.mod_id}_".encode("utf-8"), bak.read_bytes())

    def test_dat_rereset_uses_free_zombie_prefix(self):
        # 首次重置后的僵尸 + 重玩产生的新 live key
        keys = [
            f"mod_{self.mod_id}_main_n1",
            f"mod_{self.mod_id}_main_n2",
            f"MOD_{self.mod_id}_main_n1",
            f"MOD_{self.mod_id}_main_n3",
        ]
        self._write_dat(*keys)
        results = self._reset()
        self.assertEqual(results, [(self.dat, 2)])
        raw = self.dat.read_bytes()
        # live 已改成未占用前缀（mod_ 已被占用 → xod_）
        self.assertNotIn(f"MOD_{self.mod_id}_".encode("utf-8"), raw)
        self.assertIn(f"mod_{self.mod_id}_".encode("utf-8"), raw)
        self.assertIn(f"xod_{self.mod_id}_".encode("utf-8"), raw)
        # 不应出现两份相同的 mod_ 重复 live 改名（n1 只有 zombie 一份 + 原 zombie）
        self.assertEqual(raw.count(f"mod_{self.mod_id}_main_n1".encode("utf-8")), 1)
        self.assertEqual(raw.count(f"xod_{self.mod_id}_main_n1".encode("utf-8")), 1)

    def test_json_removes_live_and_zombie_keys(self):
        keys = [
            f"MOD_{self.mod_id}_main_n1",
            f"mod_{self.mod_id}_main_n2",
            f"xod_{self.mod_id}_second_a",
            "other_story_key",
            f"MOD_other_mod_main_n1",
        ]
        self._write_json(keys)
        # 无 dat 改动时仍应处理 json
        self._write_dat("unrelated")
        results = self._reset()
        self.assertEqual(results, [(self.json_path, 3)])
        data = json.loads(self.json_path.read_text(encoding="utf-8"))
        self.assertEqual(
            data["ReadStoryData"],
            ["other_story_key", "MOD_other_mod_main_n1"],
        )
        bak = self.json_path.with_suffix(self.json_path.suffix + ".lomkit_bak")
        self.assertTrue(bak.is_file())
        bak_data = json.loads(bak.read_bytes().decode("utf-8"))
        self.assertEqual(len(bak_data["ReadStoryData"]), 5)

    def test_dat_and_json_together(self):
        self._write_dat(f"MOD_{self.mod_id}_main_n1", "keep_me")
        self._write_json(
            [f"MOD_{self.mod_id}_main_n1", f"MOD_{self.mod_id}_main_n2", "keep_me"]
        )
        results = self._reset()
        by_name = {path.name: count for path, count in results}
        self.assertEqual(by_name["Save_universe.dat"], 1)
        self.assertEqual(by_name["Save_universe.json"], 2)
        data = json.loads(self.json_path.read_text(encoding="utf-8"))
        self.assertEqual(data["ReadStoryData"], ["keep_me"])

    def test_backup_not_overwritten_on_second_reset(self):
        self._write_dat(f"MOD_{self.mod_id}_main_n1")
        self._reset()
        bak = self.dat.with_suffix(self.dat.suffix + ".lomkit_bak")
        first_bak = bak.read_bytes()
        # 模拟重玩后再重置
        raw = self.dat.read_bytes()
        self.dat.write_bytes(raw + f"MOD_{self.mod_id}_main_n2".encode("utf-8"))
        self._reset()
        self.assertEqual(bak.read_bytes(), first_bak)

    def test_empty_when_no_matching_keys(self):
        self._write_dat("vanilla_only")
        self._write_json(["vanilla_only"])
        self.assertEqual(self._reset(), [])

    def test_is_mod_read_story_key_casefold_and_prefix(self):
        keys = self._keys()
        self.assertTrue(
            _is_mod_read_story_key(f"MOD_{self.mod_id}_main_n1", self.mod_id, keys)
        )
        self.assertTrue(
            _is_mod_read_story_key(f"mod_{self.mod_id}_main_n1", self.mod_id, keys)
        )
        self.assertTrue(
            _is_mod_read_story_key(f"XOD_{self.mod_id}_main_n1", self.mod_id, keys)
        )
        self.assertFalse(_is_mod_read_story_key(f"MOD_other_main_n1", self.mod_id, keys))
        self.assertFalse(_is_mod_read_story_key("short", self.mod_id, keys))
        self.assertFalse(  # type: ignore[arg-type]
            _is_mod_read_story_key(None, self.mod_id, keys)
        )

    def test_choose_zombie_skips_occupied(self):
        raw = f"mod_{self.mod_id}_a\x00xod_{self.mod_id}_b".encode("utf-8")
        chosen = _choose_zombie_prefix(raw, self.mod_id)
        self.assertEqual(chosen, f"yod_{self.mod_id}_".encode("utf-8"))

    def test_extra_ids_resets_preview_keys(self):
        preview = "lom_modkit_preview"
        self._write_dat(
            f"MOD_{self.mod_id}_main_n1",
            f"MOD_{preview}_main_n2",
            "vanilla",
        )
        self._write_json(
            [f"MOD_{self.mod_id}_main_n1", f"MOD_{preview}_main_n2", "vanilla"]
        )
        results = self._reset(extra_ids=[preview])
        raw = self.dat.read_bytes()
        self.assertNotIn(f"MOD_{self.mod_id}_".encode("utf-8"), raw)
        self.assertNotIn(f"MOD_{preview}_".encode("utf-8"), raw)
        data = json.loads(self.json_path.read_text(encoding="utf-8"))
        self.assertEqual(data["ReadStoryData"], ["vanilla"])
        # 两个 id 各改 dat 一次、各清 json 一次
        self.assertEqual(sum(n for path, n in results if path.name.endswith(".dat")), 2)
        self.assertEqual(sum(n for path, n in results if path.name.endswith(".json")), 2)

    def test_exact_keys_do_not_collide_with_longer_mod_or_node_ids(self):
        short_id = "foo"
        target = f"MOD_{short_id}_main_n1"
        longer_node = f"MOD_{short_id}_main_n10"
        longer_mod = "MOD_foo_bar_main_n1"
        embedded = "MOD_other_XMOD_foo_main_n1"
        self._write_dat(target, longer_node, longer_mod, embedded)
        self._write_json([target, longer_node, longer_mod, embedded])

        results = reset_story_read_state(
            short_id,
            saves=[self.dat],
            read_keys_by_id={short_id: [target]},
        )

        self.assertEqual(sum(count for _path, count in results), 2)
        raw = self.dat.read_bytes()
        self.assertNotIn(b"\x00" + target.encode("utf-8") + b"\x00", raw)
        self.assertIn(b"\x00mod_foo_main_n1\x00", raw)
        self.assertIn(longer_node.encode("utf-8"), raw)
        self.assertIn(longer_mod.encode("utf-8"), raw)
        self.assertIn(embedded.encode("utf-8"), raw)
        data = json.loads(self.json_path.read_text(encoding="utf-8"))
        self.assertEqual(
            data["ReadStoryData"], [longer_node, longer_mod, embedded]
        )

    def test_binaryformatter_ascii_length_prefix_is_not_mistaken_for_left_text(self):
        base = "MOD_foo_main_"
        target = base + "x" * (48 - len(base))
        self.assertEqual(len(target.encode("utf-8")), 48)
        # BinaryObjectString = record type 6 + Int32 object id + 7-bit byte length + UTF-8。
        self.dat.write_bytes(
            b"\x06" + (123).to_bytes(4, "little") + b"0" + target.encode("utf-8") + b"\x00"
        )
        results = reset_story_read_state(
            "foo",
            saves=[self.dat],
            read_keys_by_id={"foo": [target]},
        )
        self.assertEqual(results, [(self.dat, 1)])
        self.assertIn(b"mod_foo_main_", self.dat.read_bytes())

    def test_binaryformatter_key_substring_inside_other_string_is_untouched(self):
        target = "MOD_foo_main_n1"
        payload = ("note." + target).encode("utf-8")
        # 合法 BinaryObjectString，但声明长度覆盖 note.<key> 整串；目标 key
        # 只是其内部子串，不能因左侧句点被误判成独立已读记录。
        original = (
            b"\x06"
            + (123).to_bytes(4, "little")
            + bytes([len(payload)])
            + payload
            + b"\x0b"
        )
        self.dat.write_bytes(original)

        results = reset_story_read_state(
            "foo",
            saves=[self.dat],
            read_keys_by_id={"foo": [target]},
        )

        self.assertEqual(results, [])
        self.assertEqual(self.dat.read_bytes(), original)

    def test_build_story_read_keys_only_uses_say_nodes(self):
        stories = {
            "main": {
                "id": "main",
                "nodes": [
                    {"id": "say1", "type": "say", "text": "你好"},
                    {"id": "show1", "type": "show"},
                ],
            },
            "part-two": {
                "id": "part-two",
                "nodes": [{"id": "say2", "type": "say", "text": "再见"}],
            },
        }
        self.assertEqual(
            set(build_story_read_keys("foo", stories)),
            {"MOD_foo_main_say1", "MOD_foo_part-two_say2"},
        )

    def test_build_story_read_keys_rejects_invalid_legacy_ids(self):
        stories = {
            "main": {
                "id": "bad script",
                "nodes": [{"id": "say1", "type": "say", "text": "你好"}],
            }
        }
        with self.assertRaises(GameInstallError):
            build_story_read_keys("foo", stories)

    def test_requires_exact_key_map(self):
        self._write_dat(f"MOD_{self.mod_id}_main_n1")
        with self.assertRaises(GameInstallError):
            reset_story_read_state(self.mod_id, saves=[self.dat])

    def test_atomic_save_failure_preserves_dat_and_json(self):
        target = f"MOD_{self.mod_id}_main_n1"
        self._write_dat(target, "keep")
        original_dat = self.dat.read_bytes()
        with mock.patch("game_install.os.replace", side_effect=OSError("blocked")):
            with self.assertRaises(GameInstallError):
                self._reset()
        self.assertEqual(self.dat.read_bytes(), original_dat)
        self.assertFalse(list(self.slot.glob("Save_universe.dat.*.tmp")))

        self._write_dat("unrelated")
        self._write_json([target, "keep"])
        original_json = self.json_path.read_bytes()
        with mock.patch("game_install.os.replace", side_effect=OSError("blocked")):
            with self.assertRaises(GameInstallError):
                self._reset()
        self.assertEqual(self.json_path.read_bytes(), original_json)
        self.assertFalse(list(self.slot.glob("Save_universe.json.*.tmp")))


if __name__ == "__main__":
    unittest.main(verbosity=2)

