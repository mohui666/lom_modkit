"""Synthetic fixtures only: never open an installed game."""
import contextlib
import io
import json
from pathlib import Path
import sys
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import _probe_assets as probe
import preview_asset_worker as worker


class PreviewWorkerTest(unittest.TestCase):
    def fixture(self, root, platform="StandaloneWindows"):
        game = root / "游戏 安装"
        aa = game / "Mortal_Data/StreamingAssets/aa"
        bundles = aa / platform
        bundles.mkdir(parents=True)
        (aa / "catalog.json").write_text("{}")
        (game / "Mortal_Data/sharedassets2.assets").touch()
        (bundles / "pictures.bundle").touch()
        output = root / "预览 缓存"
        output.mkdir()
        addresses = probe.CatalogBundleMap()
        addresses.update({"normal": "pictures.bundle", "view": "pictures.bundle"})
        addresses.asset_paths.update({"normal": "full/player.png", "view": "full/view.png"})
        chars = {"player": {"name": "赵活", "first": "normal", "portraits": {"normal": "normal", "smile": "normal", "bad": "missing"}}}
        return game, output, addresses, chars

    def test_aliases_share_png_and_failures_never_create_broken_mappings(self):
        with TemporaryDirectory() as folder:
            game, output, addresses, chars = self.fixture(Path(folder))
            calls = []

            def sprite(env, address):
                calls.append(address)
                return SimpleNamespace(read=lambda: SimpleNamespace(image=Image.new("RGBA", (300, 150), (255, 0, 0, 128))))

            with patch.object(probe, "parse_catalog", return_value=addresses), patch.object(probe, "load_sa2", return_value=(chars, {"room": {"address": "view"}, "alias": {"address": "view"}})), patch.object(probe, "resolve_sprite", side_effect=sprite), patch("UnityPy.load", return_value=object()), contextlib.redirect_stdout(io.StringIO()) as stdout:
                report = worker.extract(game, output, 128)
            mapping = json.loads((output / "preview_map.json").read_text())
            portraits = mapping["characters"]["player"]["portraits"]
            self.assertEqual(portraits["normal"], portraits["smile"])
            self.assertNotIn("bad", portraits)
            self.assertEqual(mapping["views"]["room"], mapping["views"]["alias"])
            self.assertEqual(calls, ["full/player.png", "full/view.png"])
            self.assertEqual((report["exported"], report["failed"]), (2, 1))
            with Image.open(output / portraits["normal"]) as image:
                self.assertEqual(image.size, (128, 64))
                self.assertEqual(image.mode, "RGBA")
            self.assertEqual(json.loads(stdout.getvalue().splitlines()[-1])["event"], "result")
            # Fake game source stays untouched.
            self.assertEqual((game / "Mortal_Data/StreamingAssets/aa/catalog.json").read_text(), "{}")

    def test_rejects_game_directory_outputs_and_preserves_existing_caches(self):
        with TemporaryDirectory() as folder:
            game, output, _, _ = self.fixture(Path(folder))
            marker = output / "original.txt"
            marker.write_text("retain")
            with self.assertRaisesRegex(ValueError, "必须为空"):
                worker.extract(game, output, 128)
            self.assertEqual(marker.read_text(), "retain")
            inside = game / "cache"
            inside.mkdir()
            with self.assertRaisesRegex(ValueError, "游戏文件夹之外"):
                worker.extract(game, inside, 128)

    def test_picture_keys_follow_the_native_prefix_and_use_confined_preview_paths(self):
        with TemporaryDirectory() as folder:
            game, output, addresses, chars = self.fixture(Path(folder))
            addresses.update({"pic_big_fixture": "pictures.bundle", "pic_missing": "missing.bundle"})
            addresses.asset_paths["pic_big_fixture"] = "Assets/Images/BigPicture/fixture.png"
            sprite = SimpleNamespace(read=lambda: SimpleNamespace(image=Image.new("RGBA", (300, 150))))
            with patch.object(probe, "parse_catalog", return_value=addresses), patch.object(probe, "load_sa2", return_value=(chars, {"room": {"address": "view"}})), patch.object(probe, "resolve_sprite", return_value=sprite), patch("UnityPy.load", return_value=object()), contextlib.redirect_stdout(io.StringIO()):
                report = worker.extract(game, output, 128)
            mapping = json.loads((output / "preview_map.json").read_text())
            self.assertEqual(report["pictures"], 1)
            self.assertEqual(mapping["picture_catalog"][0], {"key": "big_fixture", "category": "BigPicture"})
            self.assertNotIn("missing", mapping["pictures"])
            relative = mapping["pictures"]["big_fixture"]
            self.assertTrue(relative.startswith("assets/pictures/"))
            self.assertTrue((output / relative).is_file())

    def test_total_failure_does_not_publish_a_preview_map(self):
        with TemporaryDirectory() as folder:
            game, output, addresses, chars = self.fixture(Path(folder))
            with patch.object(probe, "parse_catalog", return_value=addresses), patch.object(probe, "load_sa2", return_value=(chars, {"room": {"address": "missing"}})), patch("UnityPy.load", side_effect=ValueError("fixture failure")), contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaisesRegex(ValueError, "未能导出任何"):
                    worker.extract(game, output, 128)
            self.assertFalse((output / "preview_map.json").exists())

    def test_x64_game_exports_from_the_current_platform_directory(self):
        with TemporaryDirectory() as folder:
            game, output, addresses, chars = self.fixture(Path(folder), "StandaloneWindows64")
            sprite = SimpleNamespace(read=lambda: SimpleNamespace(image=Image.new("RGBA", (20, 30))))
            with patch.object(probe, "parse_catalog", return_value=addresses), patch.object(probe, "load_sa2", return_value=(chars, {"room": {"address": "view"}})), patch.object(probe, "resolve_sprite", return_value=sprite), patch("UnityPy.load", return_value=object()) as load, contextlib.redirect_stdout(io.StringIO()):
                report = worker.extract(game, output, 128)
            self.assertEqual(report["resource_platform"], "StandaloneWindows64")
            self.assertIn("StandaloneWindows64", load.call_args[0][0])
            self.assertEqual(report["exported"], 2)

    def test_missing_platform_directory_is_reported_before_loading_bundles(self):
        with TemporaryDirectory() as folder:
            root = Path(folder)
            with self.assertRaisesRegex(ValueError, "StandaloneWindows64 / StandaloneWindows"):
                probe.bundle_directory(root)


if __name__ == "__main__":
    unittest.main()
