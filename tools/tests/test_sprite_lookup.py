"""Regression for Addressables with several Sprite subassets called normal."""
from pathlib import Path
from types import ModuleType, SimpleNamespace
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
try:
    import UnityPy
    _stubbed_unity = False
except ImportError:
    sys.modules["UnityPy"] = ModuleType("UnityPy")
    _stubbed_unity = True
import extract_preview_assets as extractor
if _stubbed_unity:
    del sys.modules["UnityPy"]


ORIGINAL = "Assets/__Project/Images/Characters/Player_主角/normal.png"
BEAUTIFIED = "Assets/__Project/Images/Characters/Player_自戀/normal.png"


def sprite(marker):
    image = SimpleNamespace(save=lambda path: Path(path).write_text(marker))
    data = SimpleNamespace(m_Name="normal", image=image)
    return SimpleNamespace(type=SimpleNamespace(name="Sprite"), read=lambda: data)


class SpriteLookupTest(unittest.TestCase):
    def export(self, entries, objects, address, internal_address=None):
        env = SimpleNamespace(container=SimpleNamespace(items=lambda: entries), objects=objects)
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "portrait.bundle").touch()
            old_dir, old_cache = extractor.BUNDLE_DIR, extractor._bundle_cache
            extractor.BUNDLE_DIR = str(root)
            extractor._bundle_cache = {"portrait.bundle": env}
            try:
                target = root / "result.png"
                bundles = extractor.probe.CatalogBundleMap()
                bundles[address] = "portrait.bundle"
                if internal_address:
                    bundles.asset_paths[address] = internal_address
                error = extractor.export_sprite(bundles, address, str(target))
                return error, target.read_text() if target.exists() else None
            finally:
                extractor.BUNDLE_DIR, extractor._bundle_cache = old_dir, old_cache

    def test_same_named_portraits_follow_the_full_address(self):
        original, beautified = sprite("original"), sprite("beautified")
        entries = [(ORIGINAL, original), (BEAUTIFIED, beautified)]
        # The first normal Sprite is intentionally the other appearance.
        self.assertEqual(self.export(entries, [beautified, original], ORIGINAL), (None, "original"))
        self.assertEqual(self.export(entries, [original, beautified], BEAUTIFIED), (None, "beautified"))

    def test_exact_match_ignores_texture_subasset_and_path_case(self):
        original, wrong = sprite("original"), sprite("wrong")
        texture = SimpleNamespace(type=SimpleNamespace(name="Texture2D"))
        entries = [(ORIGINAL.lower(), texture), (ORIGINAL.lower(), original)]
        self.assertEqual(self.export(entries, [wrong, original], ORIGINAL.replace("/", "\\")), (None, "original"))

    def test_short_address_can_use_a_unique_sprite_name(self):
        only = sprite("only")
        self.assertEqual(self.export([], [only], "normal"), (None, "only"))

    def test_catalog_alias_uses_its_actual_internal_path(self):
        renamed = "Assets/__Project/Images/Characters/RenamedCharacter/normal.png"
        original, wrong = sprite("original"), sprite("wrong")
        self.assertEqual(self.export([(renamed, original)], [wrong, original], ORIGINAL, renamed), (None, "original"))

    def test_ambiguous_short_address_is_reported(self):
        error, image = self.export([], [sprite("one"), sprite("two")], "normal")
        self.assertIsNone(image)
        self.assertIn("无法唯一匹配", error)

    def test_missing_full_address_does_not_pick_a_nearby_character(self):
        error, image = self.export([(BEAUTIFIED, sprite("beautified"))], [sprite("beautified")], ORIGINAL)
        self.assertIsNone(image)
        self.assertIn("找不到资源地址", error)


if __name__ == "__main__":
    unittest.main()
