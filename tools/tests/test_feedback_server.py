import argparse
from contextlib import redirect_stdout
from http.client import HTTPConnection
import io
import json
from pathlib import Path
import sys
from tempfile import TemporaryDirectory
from threading import Thread
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from feedback_server import FeedbackServer, ROUTE, initialize, queue


class FeedbackInboxTest(unittest.TestCase):
    def test_submission_is_private_idempotent_and_can_be_triaged(self):
        with TemporaryDirectory() as folder:
            database = Path(folder) / "inbox.sqlite3"
            initialize(database)
            with FeedbackServer(0, database) as server:
                worker = Thread(target=server.serve_forever, kwargs={"poll_interval": 0.02})
                worker.start()
                try:
                    report = {
                        "schema": 1, "id": "30000000000040008000000000000001",
                        "description": "保存时出现错误",
                        "context": {
                            "editor_version": "1.6.3", "host_version": None, "game_build": 21981442,
                            "os": "windows", "architecture": "x86_64", "locale": "chs",
                            "page": "story", "project_mode": "addon", "step_type": "say",
                            "error_kind": "editor.error.ops_permission", "os_error": 5,
                        },
                    }

                    def submit(value, method="POST"):
                        connection = HTTPConnection("127.0.0.1", server.server_port, timeout=4)
                        try:
                            connection.request(
                                method, ROUTE, json.dumps(value).encode(),
                                {"Content-Type": "application/json"},
                            )
                            response = connection.getresponse()
                            return response.status, response.read()
                        finally:
                            connection.close()

                    first, response = submit(report)
                    self.assertEqual(first, 201)
                    receipt = {"id": report["id"], "number": 1}
                    self.assertEqual(json.loads(response), receipt)
                    duplicate, response = submit(report)
                    self.assertEqual(duplicate, 200)
                    self.assertEqual(json.loads(response), receipt)
                    self.assertEqual(submit({**report, "description": "改写"})[0], 409)
                    self.assertEqual(submit({**report, "path": "private"})[0], 400)
                    self.assertGreaterEqual(submit(report, "GET")[0], 400)
                finally:
                    server.shutdown()
                    worker.join()

            def run(command, **flags):
                output = io.StringIO()
                with redirect_stdout(output):
                    queue(argparse.Namespace(database=database, command=command, **flags))
                return output.getvalue()

            issues = run("list", status="open", limit=10).splitlines()
            self.assertEqual(len(issues), 1)
            first = json.loads(issues[0])
            self.assertEqual(first["number"], 1)
            self.assertEqual(first["receipt"], report["id"])
            self.assertEqual(json.loads(run("status", issue="1", status="in_progress"))["status"],
                             "in_progress")
            self.assertEqual(run("list", status="open", limit=10), "")
            resolved = json.loads(run("status", issue=report["id"], status="resolved"))
            self.assertEqual(resolved["report"], report)
            self.assertEqual(json.loads(run("show", issue="#1"))["status"], "resolved")


if __name__ == "__main__":
    unittest.main()
