"""LoM Modkit feedback inbox. Serve on loopback behind the site's HTTPS proxy."""

import argparse
from contextlib import closing
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import sqlite3
import sys


ROUTE = "/api/v1/feedback"
MAX_BODY_BYTES = 16 * 1024
MAX_DESCRIPTION = 2_000
STATUSES = ("open", "in_progress", "resolved")
CONTEXT_FIELDS = {
    "editor_version", "host_version", "game_build", "os", "architecture", "locale",
    "page", "project_mode", "step_type", "error_kind", "os_error",
}
IDENTIFIER = re.compile(r"[a-z][a-z0-9_.-]{0,63}")
RECEIPT = re.compile(r"[0-9a-f]{32}")
VERSION = re.compile(r"[0-9]+(?:\.[0-9]+){2,3}(?:-[0-9A-Za-z.-]+)?")


def validate(report):
    if (
        type(report) is not dict or report.keys() != {"schema", "id", "description", "context"}
        or type(report["schema"]) is not int or report["schema"] != 1
        or not isinstance(report["id"], str) or not RECEIPT.fullmatch(report["id"])
    ):
        raise ValueError("invalid report")
    description = report["description"]
    context = report["context"]
    if (
        not isinstance(description, str) or len(description) > MAX_DESCRIPTION
        or "\0" in description or type(context) is not dict or context.keys() != CONTEXT_FIELDS
    ):
        raise ValueError("invalid feedback fields")
    description.encode("utf-8")
    for field in ("editor_version", "host_version"):
        value = context[field]
        if field == "host_version" and value is None:
            continue
        if not isinstance(value, str) or len(value) > 48 or not VERSION.fullmatch(value):
            raise ValueError(field)
    for field in ("os", "architecture", "step_type", "error_kind"):
        value = context[field]
        if field in ("step_type", "error_kind") and value is None:
            continue
        if not isinstance(value, str) or not IDENTIFIER.fullmatch(value):
            raise ValueError(field)
    for field, choices in (
        ("locale", ("chs", "cht", "ja", "ko")),
        ("page", ("story", "materials", "game", "release")),
        ("project_mode", (None, "addon", "sandbox")),
    ):
        if context[field] not in choices:
            raise ValueError(field)
    for field, bits in (("game_build", 64), ("os_error", 32)):
        value = context[field]
        if value is not None and (type(value) is not int or not 0 <= value < 1 << bits):
            raise ValueError(field)
    if not description.strip() and context["error_kind"] is None:
        raise ValueError("empty feedback")
    return report


def now():
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def initialize(database):
    database.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    with closing(sqlite3.connect(database)) as db, db:
        db.execute("""
            CREATE TABLE IF NOT EXISTS issues (
                number INTEGER PRIMARY KEY,
                receipt TEXT NOT NULL UNIQUE,
                status TEXT NOT NULL DEFAULT 'open'
                    CHECK (status IN ('open', 'in_progress', 'resolved')),
                created_utc TEXT NOT NULL,
                updated_utc TEXT NOT NULL,
                report TEXT NOT NULL
            )
        """)


def enqueue(database, report):
    receipt = report["id"]
    stored = json.dumps(report, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    timestamp = now()
    with closing(sqlite3.connect(database)) as db, db:
        inserted = db.execute(
            "INSERT INTO issues (receipt, created_utc, updated_utc, report) VALUES (?, ?, ?, ?) "
            "ON CONFLICT(receipt) DO NOTHING",
            (receipt, timestamp, timestamp, stored),
        )
        if inserted.rowcount:
            return 201, inserted.lastrowid
        number, previous = db.execute(
            "SELECT number, report FROM issues WHERE receipt = ?", (receipt,)
        ).fetchone()
        return (200, number) if previous == stored else (409, None)


class FeedbackHandler(BaseHTTPRequestHandler):
    timeout = 8

    def log_message(self, fmt, *args):
        # Keep reporter IPs and request bodies out of the inbox service log.
        pass

    def reply(self, code, body):
        raw = json.dumps(body, separators=(",", ":")).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_POST(self):
        if self.path != ROUTE:
            self.reply(404, {"error": "not_found"})
            return
        if self.headers.get_content_type() != "application/json":
            self.reply(415, {"error": "expected_json"})
            return
        length = self.headers.get("Content-Length", "")
        if self.headers.get("Transfer-Encoding") or not length.isascii() or not length.isdecimal():
            self.reply(400, {"error": "invalid_length"})
            return
        if len(length) > 5 or not 0 < int(length) <= MAX_BODY_BYTES:
            self.reply(413, {"error": "invalid_size"})
            return
        try:
            raw = self.rfile.read(int(length))
            if len(raw) != int(length):
                raise ValueError("truncated report")
            report = validate(json.loads(raw.decode("utf-8")))
        except (ValueError, UnicodeError, RecursionError):
            self.reply(400, {"error": "invalid_report"})
            return
        except TimeoutError:
            self.reply(408, {"error": "request_timeout"})
            return
        try:
            code, number = enqueue(self.server.database, report)
        except sqlite3.Error as error:
            print(f"Feedback inbox write failed: {error}", file=sys.stderr, flush=True)
            self.reply(503, {"error": "inbox_unavailable"})
            return
        if number is None:
            self.reply(code, {"error": "id_conflict"})
        else:
            self.reply(code, {"id": report["id"], "number": number})


class FeedbackServer(ThreadingHTTPServer):
    def __init__(self, port, database, bind_address="127.0.0.1"):
        self.database = database
        super().__init__((bind_address, port), FeedbackHandler)


def queue(args):
    if not args.database.is_file():
        raise SystemExit("Feedback inbox database does not exist")
    with closing(sqlite3.connect(args.database)) as db, db:
        db.row_factory = sqlite3.Row
        if args.command == "list":
            rows = db.execute(
                "SELECT number, receipt, status, created_utc, updated_utc, report FROM issues "
                "WHERE (? IS NULL OR status = ?) ORDER BY number DESC LIMIT ?",
                (args.status, args.status, args.limit),
            )
            for row in rows:
                issue = dict(row)
                report = json.loads(issue.pop("report"))
                issue["description"] = report["description"][:120]
                issue["error_kind"] = report["context"]["error_kind"]
                print(json.dumps(issue, ensure_ascii=False))
            return
        if RECEIPT.fullmatch(args.issue):
            identity = (None, args.issue)
        elif re.fullmatch(r"#?[1-9][0-9]{0,17}", args.issue):
            identity = (int(args.issue.lstrip("#")), None)
        else:
            raise SystemExit("Invalid issue number or receipt ID")
        if args.command == "status":
            changed = db.execute(
                "UPDATE issues SET status = ?, updated_utc = ? WHERE number = ? OR receipt = ?",
                (args.status, now(), *identity),
            )
            if not changed.rowcount:
                raise SystemExit("Feedback issue not found")
            db.commit()
        row = db.execute(
            "SELECT * FROM issues WHERE number = ? OR receipt = ?", identity
        ).fetchone()
        if row is None:
            raise SystemExit("Feedback issue not found")
        issue = dict(row)
        issue["report"] = json.loads(issue["report"])
        print(json.dumps(issue, ensure_ascii=False, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--database", type=Path, required=True)
    commands = parser.add_subparsers(dest="command", required=True)
    serve = commands.add_parser("serve")
    serve.add_argument("--bind", default="127.0.0.1")
    serve.add_argument("--port", type=int, default=8892)
    recent = commands.add_parser("list")
    recent.add_argument("--status", choices=STATUSES)
    recent.add_argument("--limit", type=int, choices=range(1, 501), default=30, metavar="1-500")
    commands.add_parser("show").add_argument("issue", help="Issue number or receipt ID")
    status = commands.add_parser("status")
    status.add_argument("issue", help="Issue number or receipt ID")
    status.add_argument("status", choices=STATUSES)
    args = parser.parse_args()
    os.umask(0o077)
    if args.command == "serve":
        initialize(args.database)
        with FeedbackServer(args.port, args.database, args.bind) as server:
            print(f"Feedback inbox listening on {args.bind}:{server.server_port}", flush=True)
            server.serve_forever()
    else:
        queue(args)


if __name__ == "__main__":
    main()
