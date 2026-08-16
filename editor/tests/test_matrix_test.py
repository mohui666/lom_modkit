# -*- coding: utf-8 -*-
from pathlib import Path
import json
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))

import test_matrix


class TestMatrixTest(unittest.TestCase):
    def test_matrix_covers_every_required_lane_without_network_or_git(self):
        matrix = test_matrix.build_matrix(ROOT)
        self.assertEqual(
            [step.name for step in matrix],
            [
                "compiler-tests", "editor-tests", "editor-smoke", "editor-stress",
                "runtime-build", "runtime-smoke",
            ],
        )
        coverage = {item for step in matrix for item in step.coverage}
        self.assertTrue({
            "compiler tests", "story_api tests", "content tests", "package tests",
            "editor smoke", "editor stress", "migration tests", "localization tests",
            "watermark tests", "Runtime build", "Runtime smoke",
        }.issubset(coverage))
        commands = " ".join(part for step in matrix for part in step.command).lower()
        self.assertNotIn("git", commands)
        self.assertNotIn("http", commands)
        self.assertNotIn("publish", commands)

    def test_runner_continues_after_failure_and_writes_machine_report(self):
        matrix = test_matrix.build_matrix(ROOT)[:2]
        completed = [mock.Mock(returncode=1), mock.Mock(returncode=0)]
        with mock.patch("test_matrix.subprocess.run", side_effect=completed) as run:
            results = test_matrix.run_steps(matrix)
        self.assertEqual(run.call_count, 2)
        self.assertEqual([item.exit_code for item in results], [1, 0])
        with tempfile.TemporaryDirectory() as temp:
            report = Path(temp) / "matrix.json"
            test_matrix.write_report(report, results)
            payload = json.loads(report.read_text(encoding="utf-8"))
        self.assertFalse(payload["passed"])
        self.assertEqual([item["name"] for item in payload["steps"]],
                         ["compiler-tests", "editor-tests"])

    def test_default_cli_only_lists_and_does_not_run(self):
        with mock.patch("test_matrix.run_steps") as run:
            self.assertEqual(test_matrix.main([]), 0)
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
