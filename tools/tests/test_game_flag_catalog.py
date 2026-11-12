"""Metadata parsing stays conservative and never alters runtime flag values."""
import sys
import unittest
from types import SimpleNamespace
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from refresh_game_flag_catalog import direct_references, enrich, value_options


class GameFlagCatalogTests(unittest.TestCase):
    def test_explicit_choices_accept_game_note_formats(self):
        self.assertEqual(value_options("留學\n1: 青城\n2: 崆峒"), [
            {"value": 1, "name": "青城"}, {"value": 2, "name": "崆峒"}])
        self.assertEqual(value_options("選擇\r\n1. 幫忙\r\n2 不幫忙")[1],
                         {"value": 2, "name": "不幫忙"})
        self.assertEqual(value_options("關係\n=0，未選擇\n=1，已選擇")[0]["value"], 0)

    def test_ambiguous_and_non_state_numbers_are_not_invented_choices(self):
        self.assertEqual(value_options("事件1-4-3-5\n1.5 秒\n2147483648: 太大"), [])
        self.assertEqual(value_options("第一段\n1: 勝\n另一段\n1: 敗"), [])
        self.assertEqual(value_options("勝利代表完成，沒有寫出數值"), [])

    def test_official_names_and_unknown_flags_keep_their_identity(self):
        row = enrich({"id": "M60018"}, "開啟正心堂養成：葉雲裳彈琴", 0, "叶云裳学琴", ["chapter"])
        self.assertEqual(row["id"], "M60018")
        self.assertEqual(row["name"], "叶云裳学琴")
        self.assertEqual(row["scripts"], ["chapter"])
        self.assertNotIn("values", row)
        self.assertEqual(enrich({"id": "unknown"}, "", 7, None, []),
                         {"id": "unknown", "name": "unknown", "default": 7})

    def test_temporary_notes_are_marked_without_reinterpreting_their_values(self):
        row = enrich({"id": "A00001"}, "旗標暫存1", 0, None, [])
        self.assertTrue(row["temporary"])
        self.assertEqual(row["name"], "旗標暫存1")
        self.assertNotIn("values", row)

    def test_references_are_direct_calls_from_original_chapters_only(self):
        class Text:
            type = SimpleNamespace(name="TextAsset")
            def read(self):
                return SimpleNamespace(m_Name="chapter", m_Script='''
statmodifymanager.SetFlag("one", 1)
-- statmodifymanager.SetFlag("comment", 1)
--[[ luamanager.GetFlagData("long_comment") ]]
local value = luamanager.GetFlagData('one')
statmodifymanager.AddFlag("two", 1)
''')
        self.assertEqual(direct_references({"story/chinesetraditional/chapter": Text(),
                                           "story/chinesesimplified/other": Text()}),
                         {"one": ["chapter"], "two": ["chapter"]})


if __name__ == "__main__":
    unittest.main()
