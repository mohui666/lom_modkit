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

    def test_confirmed_meeting_flags_are_named_without_overriding_official_notes(self):
        flag = {"id": "Meeting_Option_101_01"}
        row = enrich(flag, "", 0, None, ["meeting_option_101_01"])
        self.assertEqual(row["name"], "已開發殺人撲滿")
        self.assertEqual(row["description_source"], "story")
        self.assertEqual(row["default"], 0)
        self.assertEqual(row["values"], [{"value": 0, "name": "否"}, {"value": 1, "name": "是"}])
        self.assertEqual(row["scripts"], ["meeting_option_101_01"])
        official = enrich(flag, "原版用途\n1：完成", 0, "原版名稱", [])
        self.assertEqual(official["name"], "原版名稱")
        self.assertNotIn("description_source", official)
        self.assertEqual(official["values"], [{"value": 1, "name": "完成"}])

    def test_event_database_metadata_survives_refresh(self):
        previous = {
            "id": "P_Alchemy_01",
            "name": "煉丹房開放狀態",
            "description": "管控煉丹房的入口。",
            "description_source": "event_database",
            "values": [{"value": 0, "name": "開放"}, {"value": 1, "name": "關閉"}],
            "values_source": "event_database",
        }
        saved = enrich(previous, "", 0, None, ["ch0_2"])
        self.assertEqual(saved, {**previous, "default": 0, "scripts": ["ch0_2"]})
        official = enrich(previous, "原版用途\n0：開放\n1：封鎖", 0, "遊戲名稱", [])
        self.assertEqual(official["name"], "遊戲名稱")
        self.assertEqual(official["values"][1]["name"], "封鎖")
        self.assertNotIn("description_source", official)
        self.assertNotIn("values_source", official)

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
