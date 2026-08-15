# -*- coding: utf-8 -*-
import json
import os
import tempfile
import unittest
import zipfile

from lomc import LomcError, apply_story_locale, compile_story, pack_mod, validate_story


def story(localized=True):
    value = {
        "id": "main", "title": "中文章", "start": "say1", "nodes": [
            {"id": "say1", "type": "say", "character": "player", "text": "你好"},
            {"id": "choice1", "type": "choice", "options": [
                {"text": "继续", "goto": "msg1"}, {"text": "结束", "goto": "end1"},
            ]},
            {"id": "msg1", "type": "message", "text": "提示", "goto": "end1"},
            {"id": "end1", "type": "end"},
        ],
    }
    if localized:
        value["localization"] = {
            "default_locale": "zh_CN", "fallback_locale": "zh_TW",
            "translations": {
                "zh_TW": {"story.title": "繁中章", "say1.text": "你好呀", "choice1.options.0.text": "繼續", "msg1.text": "提示繁"},
                "ja": {"story.title": "日本語章", "say1.text": "こんにちは", "choice1.options.0.text": "続ける"},
            },
        }
    return value


class StoryLocalizationTest(unittest.TestCase):
    def test_apply_requested_fallback_and_source_without_mutating_story(self):
        original = story()
        ja = apply_story_locale(original, "ja")
        self.assertEqual(ja["title"], "日本語章")
        self.assertEqual(ja["nodes"][0]["text"], "こんにちは")
        self.assertEqual(ja["nodes"][1]["options"][0]["text"], "続ける")
        self.assertEqual(ja["nodes"][2]["text"], "提示繁")  # ja missing → zh_TW fallback
        self.assertEqual(ja["nodes"][1]["options"][1]["text"], "结束")  # fallback missing → source
        self.assertIn("localization", original)
        self.assertNotIn("localization", ja)

    def test_compile_locale_localizes_non_say_literals(self):
        lua = compile_story(story(), locale="ja")
        self.assertIn("続ける", lua)
        self.assertIn("提示繁", lua)
        self.assertNotIn("继续", lua)

    def test_custom_intro_body_uses_text_field(self):
        value = story()
        value["nodes"].insert(-1, {"id": "intro1", "type": "intro", "intro_source": "custom", "name": "侠客", "text": "人物介绍"})
        value["localization"]["translations"]["ja"].update({"intro1.name": "侠客JP", "intro1.text": "紹介文"})
        localized = apply_story_locale(value, "ja")
        intro = next(node for node in localized["nodes"] if node["id"] == "intro1")
        self.assertEqual((intro["name"], intro["text"]), ("侠客JP", "紹介文"))

    def test_validation_rejects_unknown_locale_path_and_default_duplicate(self):
        bad = story(); bad["localization"]["translations"]["fr"] = {}
        with self.assertRaisesRegex(LomcError, "不支持"):
            validate_story(bad)
        bad = story(); bad["localization"]["translations"]["ja"]["missing.text"] = "x"
        with self.assertRaisesRegex(LomcError, "不存在"):
            validate_story(bad)
        bad = story(); bad["localization"]["translations"]["zh_CN"] = {"say1.text": "重复"}
        with self.assertRaisesRegex(LomcError, "默认语言"):
            validate_story(bad)

    def _make_mod(self, folder, localized=True):
        manifest = {"format": 1, "id": "locdemo", "name": "Loc", "version": "1", "author": "A", "description": "D", "entry": "main"}
        os.makedirs(os.path.join(folder, "story"))
        with open(os.path.join(folder, "manifest.json"), "w", encoding="utf-8") as handle: json.dump(manifest, handle, ensure_ascii=False)
        with open(os.path.join(folder, "story", "main.json"), "w", encoding="utf-8") as handle: json.dump(story(localized), handle, ensure_ascii=False)

    def test_pack_writes_default_legacy_and_four_locale_variants(self):
        with tempfile.TemporaryDirectory() as root:
            mod_dir = os.path.join(root, "mod"); os.makedirs(mod_dir); self._make_mod(mod_dir)
            output = pack_mod(mod_dir)
            with zipfile.ZipFile(output) as archive:
                names = set(archive.namelist())
                self.assertIn("lua/main.lua", names)
                self.assertIn("texts.json", names)
                self.assertIn("localization.json", names)
                for locale in ("zh_CN", "zh_TW", "ja", "ko"):
                    self.assertIn("lua/%s/main.lua" % locale, names)
                    self.assertIn("texts/%s.json" % locale, names)
                meta = json.loads(archive.read("localization.json"))
                self.assertEqual((meta["default_locale"], meta["fallback_locale"]), ("zh_CN", "zh_TW"))
                ja_texts = json.loads(archive.read("texts/ja.json"))
                self.assertEqual(ja_texts["MOD_locdemo_main_say1"], "こんにちは")
                ko_texts = json.loads(archive.read("texts/ko.json"))
                self.assertEqual(ko_texts["MOD_locdemo_main_say1"], "你好呀")
                self.assertIn("続ける", archive.read("lua/ja/main.lua").decode("utf-8"))

    def test_old_story_package_has_no_migration_artifacts(self):
        with tempfile.TemporaryDirectory() as root:
            mod_dir = os.path.join(root, "mod"); os.makedirs(mod_dir); self._make_mod(mod_dir, False)
            output = pack_mod(mod_dir)
            with zipfile.ZipFile(output) as archive:
                self.assertNotIn("localization.json", archive.namelist())
                self.assertNotIn("lua/zh_CN/main.lua", archive.namelist())
                self.assertEqual(json.loads(archive.read("texts.json"))["MOD_locdemo_main_say1"], "你好")


if __name__ == "__main__": unittest.main()
