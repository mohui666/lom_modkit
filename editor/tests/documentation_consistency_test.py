# -*- coding: utf-8 -*-
"""Focused documentation contracts for the feature-breadth sprint."""

from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parents[2]
EDITOR = ROOT / "editor"
if str(EDITOR) not in sys.path:
    sys.path.insert(0, str(EDITOR))

import models  # noqa: E402


README_FILES = (
    ROOT / "README.md",
    ROOT / "README.cht.md",
    ROOT / "README.ja.md",
    ROOT / "README.ko.md",
)
DOC_INDEXES = (
    ROOT / "docs" / "README.md",
    ROOT / "docs" / "cht" / "README.md",
    ROOT / "docs" / "ja" / "README.md",
    ROOT / "docs" / "ko" / "README.md",
)
CAPABILITY_DOCS = (
    ROOT / "docs" / "chs" / "current_capabilities.md",
    ROOT / "docs" / "cht" / "current_capabilities.md",
    ROOT / "docs" / "ja" / "current_capabilities.md",
    ROOT / "docs" / "ko" / "current_capabilities.md",
)
LINK_RE = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


class DocumentationConsistencyTest(unittest.TestCase):
    def test_public_node_count_matches_schema(self) -> None:
        count = len(models.NODE_SCHEMAS)
        self.assertEqual(count, 63)
        for path in README_FILES + DOC_INDEXES + CAPABILITY_DOCS:
            text = _read(path)
            self.assertIn(str(count), text, str(path.relative_to(ROOT)))
            self.assertNotRegex(text, r"43\s*(?:种|種|種の|종)")

    def test_index_and_readme_relative_links_exist(self) -> None:
        for path in README_FILES + DOC_INDEXES:
            for raw_target in LINK_RE.findall(_read(path)):
                target = raw_target.strip().split("#", 1)[0]
                if not target or "://" in target or target.startswith("mailto:"):
                    continue
                resolved = (path.parent / unquote(target)).resolve()
                self.assertTrue(
                    resolved.exists(),
                    "%s -> %s" % (path.relative_to(ROOT), raw_target),
                )

    def test_capability_boundary_tracks_gameplay_wrappers(self) -> None:
        required = (
            "enemy", "battle_skill", "Combat", "Battle", "combat", "reward",
            "custom_shop", "mod_quest", "persistent_var", "result_screen",
        )
        for path in CAPABILITY_DOCS:
            text = _read(path)
            for term in required:
                self.assertIn(term, text, "%s: %s" % (path.relative_to(ROOT), term))

        authoritative = _read(CAPABILITY_DOCS[0])
        self.assertIn("高层 `combat`", authoritative)
        self.assertIn("尚未实机验证", authoritative)
        self.assertIn("draw", authoritative)
        self.assertIn("不写存档", authoritative)
        self.assertIn("不是数字签名", authoritative)
        self.assertNotIn("更多用户内容类型（背景等）", _read(ROOT / "README.md"))

    def test_runtime_documentation_names_fail_closed_boundary(self) -> None:
        text = _read(ROOT / "runtime" / "MortalModHost" / "README.md")
        self.assertIn("原版战斗 API", text)
        self.assertIn("不包含自定义 Battle Engine", text)
        self.assertIn("Title / Free", text)
        self.assertIn("不会继续无标播放", text)


if __name__ == "__main__":
    unittest.main()
