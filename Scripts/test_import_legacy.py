#!/usr/bin/env python3
"""The one-off import's test: a v5 home in, zero diffs out.

Parity item 4's forward path is `Scripts/import-legacy.py`, and the cutover
deletes the Swift tree that wrote the v5 file. The tree carries no committed v5
home, so this test builds the smallest one the script reads in a temp directory,
runs the import against it, and asserts the script's own verification: every
ticket by ref, status, assignee, title and `createdAt`, the session and sprint
counts, and the usage totals.

Run it with `python3 Scripts/test_import_legacy.py` from the repository root.
"""

import json
import sqlite3
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
IMPORT = ROOT / "Scripts" / "import-legacy.py"

V5_SCHEMA = """
CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, appliedAt TEXT NOT NULL);
INSERT INTO schema_migrations VALUES (1, '2026-10-01T00:00:00Z');
INSERT INTO schema_migrations VALUES (5, '2026-10-02T00:00:00Z');

CREATE TABLE lanes (
  name TEXT PRIMARY KEY,
  key TEXT NOT NULL,
  lastNumber INTEGER,
  position INTEGER NOT NULL
);
INSERT INTO lanes VALUES ('Tandem', 'TAN', 2, 0);

CREATE TABLE tickets (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  description TEXT,
  status TEXT NOT NULL,
  archived INTEGER NOT NULL,
  lane TEXT,
  number INTEGER,
  origin TEXT NOT NULL,
  assignee TEXT,
  link TEXT,
  position INTEGER NOT NULL,
  createdAt TEXT NOT NULL,
  completedAt TEXT
);
INSERT INTO tickets VALUES
  ('ticket-one', 'The open ticket', 'Body one.', 'ready', 0, 'Tandem', 1,
   'human', 'builder', NULL, 0, '2026-10-03T10:00:00Z', NULL),
  ('ticket-two', 'The done ticket', NULL, 'done', 0, 'Tandem', 2,
   'refiner', NULL, NULL, 1, '2026-10-03T11:00:00Z', '2026-10-04T12:00:00Z');

CREATE TABLE ticket_dependencies (
  ticket_id TEXT NOT NULL,
  depends_on_id TEXT NOT NULL
);
INSERT INTO ticket_dependencies VALUES ('ticket-one', 'ticket-two');

CREATE TABLE comments (
  ticketId TEXT NOT NULL,
  author TEXT,
  at TEXT NOT NULL,
  body TEXT NOT NULL
);
INSERT INTO comments VALUES ('ticket-one', 'reviewer', '2026-10-05T09:00:00Z', 'A comment.');

CREATE TABLE sessions (
  id TEXT PRIMARY KEY,
  project TEXT NOT NULL,
  agent TEXT,
  brief TEXT NOT NULL,
  status TEXT NOT NULL,
  outcome TEXT,
  summary TEXT,
  startedAt TEXT NOT NULL,
  endedAt TEXT,
  tokensInput INTEGER,
  tokensOutput INTEGER,
  tokensReasoning INTEGER,
  tokensCacheRead INTEGER,
  tokensCacheWrite INTEGER,
  seconds INTEGER,
  cost REAL,
  model TEXT,
  tickets TEXT NOT NULL
);
INSERT INTO sessions VALUES
  ('session-one', 'tandem', 'builder', 'Build it.', 'ended', 'succeeded',
   'It landed.', '2026-10-05T10:00:00Z', '2026-10-05T11:00:00Z',
   100, 200, 50, 10, 5, 3600, 0.25, 'deepseek-v4.1-flash', '["ticket-one"]');

CREATE TABLE sprints (
  lane TEXT NOT NULL,
  number INTEGER NOT NULL,
  session TEXT,
  mode TEXT NOT NULL,
  batch TEXT NOT NULL,
  runs TEXT NOT NULL,
  close TEXT,
  status TEXT NOT NULL,
  openedAt TEXT NOT NULL,
  closedAt TEXT,
  PRIMARY KEY (lane, number)
);
INSERT INTO sprints VALUES
  ('Tandem', 1, 'session-one', 'serial', '["ticket-one"]', '[]', NULL,
   'open', '2026-10-05T10:00:00Z', NULL);

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT INTO settings VALUES ('theme', 'system');
"""


def build_home(source: Path) -> None:
    connection = sqlite3.connect(source / "tandem.db")
    try:
        connection.executescript(V5_SCHEMA)
        connection.commit()
    finally:
        connection.close()
    bikeshed = source / "bikeshed" / "projects" / "tandem"
    bikeshed.mkdir(parents=True)
    (bikeshed / "tandem.md").write_text("# Tandem\n\nA note.\n", encoding="utf-8")


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="tandem-import-") as directory:
        root = Path(directory)
        source = root / "v5-home"
        target = root / "imported-home"
        source.mkdir()
        build_home(source)

        result = subprocess.run(
            [sys.executable, str(IMPORT), "--source", str(source), "--target", str(target)],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(result.stdout, file=sys.stderr)
            print(result.stderr, file=sys.stderr)
            return 1
        if "verification: zero diffs" not in result.stderr:
            print(f"the import did not report zero diffs: {result.stderr}", file=sys.stderr)
            return 1

        report = json.loads(result.stdout)
        if report["verification"] != {"ok": True, "diffs": []}:
            print(f"the report carries diffs: {report['verification']}", file=sys.stderr)
            return 1
        if report["rows"] != {
            "lanes": 1,
            "tickets": 2,
            "edges": 1,
            "comments": 1,
            "sessions": 1,
            "sprints": 1,
            "settings": 1,
        }:
            print(f"the row counts are wrong: {report['rows']}", file=sys.stderr)
            return 1
        if not (target / "tandem.db").is_file():
            print("the target has no tandem.db", file=sys.stderr)
            return 1
        if not (target / "legacy.db").is_file():
            print("the target kept no legacy.db", file=sys.stderr)
            return 1
        if not (target / "bikeshed" / "projects" / "tandem" / "tandem.md").is_file():
            print("the bikeshed was not copied", file=sys.stderr)
            return 1

        connection = sqlite3.connect(target / "tandem.db")
        try:
            row = connection.execute(
                "SELECT status, assignee, title, created_at FROM tickets WHERE id = 'ticket-one'"
            ).fetchone()
            if row != ("ready", "builder", "The open ticket", "2026-10-03T10:00:00Z"):
                print(f"the open ticket did not land: {row}", file=sys.stderr)
                return 1
            author = connection.execute("SELECT author FROM comments").fetchone()
            if author != ("reviewer",):
                print(f"the comment lost its author: {author}", file=sys.stderr)
                return 1
            source_copy = sqlite3.connect(target / "legacy.db")
            try:
                old = source_copy.execute("SELECT COUNT(*) FROM tickets").fetchone()[0]
            finally:
                source_copy.close()
            if old != 2:
                print(f"legacy.db is not the source copy: {old} tickets", file=sys.stderr)
                return 1
        finally:
            connection.close()

    print("import-legacy: zero diffs on a v5 home")
    return 0


if __name__ == "__main__":
    sys.exit(main())
