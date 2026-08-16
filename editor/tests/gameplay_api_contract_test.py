# -*- coding: utf-8 -*-
"""Offline structure checks for the recorded reverse-engineering evidence."""

from __future__ import annotations

import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / "research" / "gameplay_api_contract.json"


class GameplayApiContractTest(unittest.TestCase):
    def setUp(self) -> None:
        self.contract = json.loads(CONTRACT.read_text(encoding="utf-8"))

    def test_evidence_is_content_addressed_and_bounded(self) -> None:
        self.assertEqual(self.contract["format"], 1)
        self.assertEqual(len(self.contract["assemblies"]), 4)
        for assembly in self.contract["assemblies"]:
            self.assertRegex(assembly["file"], r"^Mortal\.[A-Za-z]+\.dll$")
            self.assertGreater(assembly["size"], 0)
            self.assertRegex(assembly["sha256"], r"^[0-9A-F]{64}$")
        self.assertGreaterEqual(len(self.contract["probes"]), 10)

    def test_result_claims_match_recorded_control_flow(self) -> None:
        caps = self.contract["capabilities"]
        self.assertEqual(caps["combat_result_win_lose"], "verified_decompile")
        self.assertEqual(caps["battle_result_friend_enemy_win"], "verified_decompile")
        self.assertEqual(caps["battle_draw_escape"], "not_exposed")
        battle = next(p for p in self.contract["probes"] if p["type"].endswith("GameLevelManager"))
        joined = "\n".join(battle["required_fragments"])
        self.assertIn("FriendWin", joined)
        self.assertIn("EnemyWin", joined)
        self.assertIn("PlayerDie, finish: false", joined)

    def test_current_low_level_codegen_uses_verified_api_names(self) -> None:
        source = (ROOT / "compiler" / "lomc" / "codegen.py").read_text(encoding="utf-8")
        for name in (
            "ModifyEnemyTeam",
            "ModifyEnemyLevel",
            "ModifyEnemyPeople",
            "ModifyEnemyId",
            "SetPlayerBattleSkill",
            "SetBattleSkillActive",
            "ResetBattleSkill",
            "ChangeScene",
        ):
            self.assertTrue(re.search(r"\b%s\b" % name, source), name)

    def test_research_doc_preserves_verification_boundaries(self) -> None:
        text = (ROOT / "research" / "gameplay_api.md").read_text(encoding="utf-8")
        self.assertIn("DECOMPILE VERIFIED", text)
        self.assertIn("AUTO VERIFIED", text)
        self.assertIn("NOT VERIFIED IN GAME", text)
        self.assertIn("不动态造地图、Prefab 或 AI", text)
        self.assertIn("不修改 GameSave schema", text)


if __name__ == "__main__":
    unittest.main()
