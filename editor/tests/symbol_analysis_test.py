"""Pure project-flow analysis, independent of the editor GUI."""

import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import symbol_analysis


class SymbolAnalysisTest(unittest.TestCase):
    def test_shared_graph_preserves_cross_chapter_reads_and_rebuilds_after_edit(self):
        stories = {
            "main": {"start": "write", "nodes": [
                {"id": "write", "type": "flag", "flag": "READY"},
                {"id": "next", "type": "end", "next_script": "second"},
            ]},
            "second": {"start": "read", "nodes": [
                {"id": "read", "type": "branch", "flag": "READY", "cases": [{"value": 1, "goto": "other"}]},
                {"id": "other", "type": "branch", "flag": "MISSING", "cases": [{"value": 1, "goto": "end"}]},
                {"id": "end", "type": "end"},
            ]},
        }
        manifest = {"entry": "main"}
        with patch.object(symbol_analysis, "analyze_story", wraps=symbol_analysis.analyze_story) as analyze:
            reports = {item.name: item for item in symbol_analysis.analyze_symbols(stories, manifest)}
        self.assertEqual(analyze.call_count, len(stories))
        self.assertFalse(reports["READY"].possibly_read_before_write)
        self.assertTrue(reports["MISSING"].possibly_read_before_write)
        stories["main"]["start"] = "next"
        reports = {item.name: item for item in symbol_analysis.analyze_symbols(stories, manifest)}
        self.assertTrue(reports["READY"].possibly_read_before_write)


if __name__ == "__main__":
    unittest.main()
