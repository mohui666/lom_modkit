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


def sprite_data(image, rect=None, offset=(0, 0)):
    width, height = rect or image.size
    return SimpleNamespace(image=image, m_Rect=SimpleNamespace(width=width, height=height),
                           m_RD=SimpleNamespace(textureRectOffset=SimpleNamespace(x=offset[0], y=offset[1])),
                           m_SpriteAtlas=None, m_AtlasTags=[], m_RenderDataKey="fixture", assets_file=None)


class PreviewWorkerTest(unittest.TestCase):
    def setUp(self):
        patcher = patch.object(worker, "extract_moods", return_value={})
        patcher.start()
        self.addCleanup(patcher.stop)
        book = patch.object(worker, "extract_event_book", return_value="assets/ui/event_book.png")
        book.start()
        self.addCleanup(book.stop)

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
                return SimpleNamespace(read=lambda: sprite_data(Image.new("RGBA", (300, 150), (255, 0, 0, 128))))

            with patch.object(probe, "parse_catalog", return_value=addresses), patch.object(probe, "load_sa2", return_value=(chars, {"room": {"address": "view"}, "alias": {"address": "view"}})), patch.object(probe, "resolve_sprite", side_effect=sprite), patch("UnityPy.load", return_value=object()), contextlib.redirect_stdout(io.StringIO()) as stdout:
                report = worker.extract(game, output, 128)
            mapping = json.loads((output / "preview_map.json").read_text())
            self.assertEqual(mapping["portrait_canvas"], "sprite_rect")
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
            sprite = SimpleNamespace(read=lambda: sprite_data(Image.new("RGBA", (300, 150))))
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
            sprite = SimpleNamespace(read=lambda: sprite_data(Image.new("RGBA", (20, 30))))
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

    def test_trimmed_normal_and_untrimmed_emotion_export_the_same_portrait_canvas(self):
        with TemporaryDirectory() as folder:
            game, output, addresses, chars = self.fixture(Path(folder))
            addresses["smile"] = "pictures.bundle"
            addresses.asset_paths["smile"] = "full/smile.png"
            chars["player"]["portraits"]["smile"] = "smile"
            cropped = Image.new("RGBA", (60, 80), (255, 0, 0, 128))
            full = Image.new("RGBA", (100, 160))
            full.paste(cropped, (15, 73))  # bottom offset = 160 - 73 - 80 = 7
            sources = {"full/player.png": sprite_data(cropped, (100, 160), (15, 7)),
                       "full/smile.png": sprite_data(full), "full/view.png": sprite_data(cropped)}
            with patch.object(probe, "parse_catalog", return_value=addresses), patch.object(probe, "load_sa2", return_value=(chars, {"room": {"address": "view"}})), patch.object(probe, "resolve_sprite", side_effect=lambda env, address: SimpleNamespace(read=lambda: sources[address])), patch("UnityPy.load", return_value=object()), contextlib.redirect_stdout(io.StringIO()):
                worker.extract(game, output, 128)
            mapping = json.loads((output / "preview_map.json").read_text())
            portraits = mapping["characters"]["player"]["portraits"]
            with Image.open(output / portraits["normal"]) as normal, Image.open(output / portraits["smile"]) as smile:
                self.assertEqual(normal.size, (80, 128))
                self.assertEqual(normal.tobytes(), smile.tobytes())
            restored = worker.portrait_canvas_image(sources["full/player.png"])
            self.assertEqual(restored.getpixel((15, 73)), (255, 0, 0, 128))
            self.assertEqual(restored.getpixel((0, 0)), (0, 0, 0, 0))

    def test_character_and_background_using_the_same_address_keep_separate_framing(self):
        with TemporaryDirectory() as folder:
            game, output, addresses, chars = self.fixture(Path(folder))
            source = sprite_data(Image.new("RGBA", (60, 80), "red"), (100, 160), (15, 7))
            sprite = SimpleNamespace(read=lambda: source)
            with patch.object(probe, "parse_catalog", return_value=addresses), patch.object(probe, "load_sa2", return_value=(chars, {"room": {"address": "normal"}})), patch.object(probe, "resolve_sprite", return_value=sprite), patch("UnityPy.load", return_value=object()), contextlib.redirect_stdout(io.StringIO()):
                worker.extract(game, output, 128)
            mapping = json.loads((output / "preview_map.json").read_text())
            portrait = mapping["characters"]["player"]["portraits"]["normal"]
            with Image.open(output / portrait) as image, Image.open(output / mapping["views"]["room"]) as view:
                self.assertEqual(image.size, (80, 128))
                self.assertEqual(view.size, (60, 80))
                self.assertEqual(view.tobytes(), source.image.tobytes())

    def test_atlas_pointer_and_tag_lookup_keep_unitypy_rotation_and_atlas_padding(self):
        from UnityPy.classes.generated import SpriteAtlasData
        from UnityPy.enums import ClassIDType
        from UnityPy.export import SpriteHelper

        class UnityPySprite(SimpleNamespace):
            @property
            def image(self):
                return SpriteHelper.get_image_from_sprite(self)

        texture = Image.new("RGBA", (3, 2))
        texture.putdata([(10 + i, 20 + i, 30 + i, 128) for i in range(6)])
        # packed + rectangular packing + 90-degree packing rotation
        data = SpriteAtlasData(alphaTexture=None, downscaleMultiplier=1, settingsRaw=1 | 2 | (4 << 2),
                               texture=None, textureRect=SimpleNamespace(x=0, y=0, width=3, height=2),
                               textureRectOffset=SimpleNamespace(x=1, y=2), uvTransform=None)
        atlas = SimpleNamespace(m_Name="chosen", m_RenderDataMap=[("fixture", data)])
        wrong = SimpleNamespace(m_Name="other", m_RenderDataMap=[])
        expected = Image.new("RGBA", (8, 9))
        oriented = texture.transpose(Image.Transpose.ROTATE_270).transpose(Image.Transpose.FLIP_TOP_BOTTOM)
        expected.paste(oriented, (1, 4))
        for direct in (True, False):
            with self.subTest(direct_pointer=direct):
                fields = vars(sprite_data(texture, (8, 9)))
                fields.pop("image")
                fields["m_SpriteAtlas"] = SimpleNamespace(read=lambda: atlas) if direct else None
                fields["m_AtlasTags"] = [] if direct else ["chosen"]
                fields["assets_file"] = SimpleNamespace(objects={1: SimpleNamespace(type=ClassIDType.SpriteAtlas, read=lambda: wrong),
                                                                 2: SimpleNamespace(type=ClassIDType.SpriteAtlas, read=lambda: atlas)})
                with patch.object(SpriteHelper, "get_image", return_value=texture):
                    restored = worker.portrait_canvas_image(UnityPySprite(**fields))
                self.assertEqual(restored.tobytes(), expected.tobytes())

    def test_legacy_exporter_restores_only_portraits(self):
        import extract_preview_assets as legacy

        with TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "fixture.bundle").touch()
            source = sprite_data(Image.new("RGBA", (60, 80), "red"), (100, 160), (15, 7))
            sprite = SimpleNamespace(read=lambda: source)
            with patch.object(legacy, "BUNDLE_DIR", str(root)), patch.object(legacy, "_bundle_cache", {}), patch.object(probe, "resolve_sprite", return_value=sprite), patch("UnityPy.load", return_value=object()):
                self.assertIsNone(legacy.export_sprite({"fixture": "fixture.bundle"}, "fixture", str(root / "portrait.png"), portrait=True))
                self.assertIsNone(legacy.export_sprite({"fixture": "fixture.bundle"}, "fixture", str(root / "view.png")))
            with Image.open(root / "portrait.png") as portrait, Image.open(root / "view.png") as view:
                self.assertEqual(portrait.size, (100, 160))
                self.assertEqual(view.size, (60, 80))


if __name__ == "__main__":
    unittest.main()
