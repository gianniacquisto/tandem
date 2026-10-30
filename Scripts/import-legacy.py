#!/usr/bin/env python3
"""One-shot import: this Mac's legacy Tandem home into the Rust store.

PRT-18, the migration `docs/port.md` describes. **Not a shipped verb**: it runs
once per home, is kept for the record and deleted after use: a `tandem import`
verb would be a v5 reader shipped forever for one run.

What it does:

  1. copies the old home's `tandem.db` (and its journal) into a staging file,
     replays the journal and checkpoints it into one file, and keeps that copy
     as `<target>/legacy.db`. **The source is only ever read**;
  2. creates the target store's schema from `rust/core/src/store/schema.rs`, the
     DDL it carries, applied in file order, with `schema_migrations` stamped to
     the version that file declares. No schema lives in this script;
  3. writes the rows the old home holds into that store: tickets with their refs,
     timestamps, origin, lane, archived flag and position; the dependency edges;
     comments and their authors; sessions with their usage (and `skills` when
     both homes carry the column); sprints with their batch and runs; lanes;
     settings. Every mutation row's `author` is `import-legacy`, because the
     import is the write;
  4. copies `<source>/bikeshed` to `<target>/bikeshed` byte for byte, scaffold,
     daily notes and `.raw/` included: the notes are file text, not schema;
  5. verifies the result: every ticket by ref, status, assignee, title and
     `createdAt`, the session and sprint counts, and the usage totals. Any diff
     is printed and exits non-zero.

The schema check is the point of reading `schema.rs` rather than shipping a
second copy of the DDL: the store adapter owns the tables, and this script
cannot drift from them. A column the target schema does not define yet (the
additive `skills` bump, PRT-6/TAN-241) is simply not written; a source column it
does not have is left NULL.

Usage:
    Scripts/import-legacy.py --source <old home> --target <fresh home>
                             [--schema <path/to/schema.rs>]

`--source` is a directory holding `tandem.db` and `bikeshed/`; `--target` is a
directory that does not exist yet (or is empty). The real home is never named
here: a run is pointed at it explicitly, and the acceptance run points both ends
at copies.
"""

import argparse
import json
import math
import re
import shutil
import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path

# Every row this script writes is authored by the import, not by a pass: the
# actor column records who made the write (docs/port.md, Store).
ACTOR = "import-legacy"

# The schema's own version declaration, so the stamp is read, not guessed.
VERSION_DECLARATION = re.compile(r"CURRENT_VERSION\s*:\s*i64\s*=\s*(\d+)\s*;")
# A Rust string literal in schema.rs. The DDL is plain multi-line text there.
RUST_LITERAL = re.compile(r'"((?:[^"\\]|\\.)*)"', re.S)
# The statements this script re-applies: the versioned shape, never the data a
# migration derives (the import writes final rows, so its backfills have nothing
# left to do).
SHAPE_STATEMENT = re.compile(r"^(CREATE\s+(TABLE|(UNIQUE\s+)?INDEX)|ALTER\s+TABLE)\b", re.I)
ESCAPES = {"n": "\n", "t": "\t", "r": "\r", '"': '"', "\\": "\\", "0": "\0"}


class ImportError(Exception):
    pass


def rust_unescape(text):
    """A Rust string literal's body with its escapes resolved."""
    out = []
    index = 0
    while index < len(text):
        character = text[index]
        if character == "\\" and index + 1 < len(text):
            index += 1
            out.append(ESCAPES.get(text[index], text[index]))
        else:
            out.append(character)
        index += 1
    return "".join(out)


def read_schema(path):
    """The target version and the DDL `schema.rs` defines, in file order."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise ImportError(f"could not read the store schema at {path}: {error}")
    declared = VERSION_DECLARATION.search(text)
    if declared is None:
        raise ImportError(f"{path} declares no CURRENT_VERSION")
    statements = []
    for match in RUST_LITERAL.finditer(text):
        body = rust_unescape(match.group(1)).strip()
        if SHAPE_STATEMENT.match(body):
            statements.append(body)
    if not statements:
        raise ImportError(f"{path} carries no DDL to apply")
    return int(declared.group(1)), statements


def table_columns(connection, table):
    rows = connection.execute(f"PRAGMA table_info({table})").fetchall()
    return [row[1] for row in rows]


def create_store(path, version, statements, schema_path, applied_at):
    """The target store: the schema's shape, stamped with its own version."""
    connection = sqlite3.connect(path)
    try:
        for statement in statements:
            try:
                connection.executescript(statement)
            except sqlite3.Error as error:
                raise ImportError(
                    f"the DDL from {schema_path} failed: {error}\n{statement}"
                )
        tables = [
            row[0]
            for row in connection.execute(
                "SELECT name FROM sqlite_master WHERE type = 'table'"
            )
        ]
        if "schema_migrations" not in tables:
            raise ImportError(f"{schema_path} defines no schema_migrations seam")
        connection.executemany(
            "INSERT OR IGNORE INTO schema_migrations (version, applied_at, author)"
            " VALUES (?, ?, ?)",
            [(number, applied_at, ACTOR) for number in range(1, version + 1)],
        )
        connection.commit()
    finally:
        connection.close()


def stage_legacy(source_database, target):
    """A copy of the old database, journal replayed, at `<target>/legacy.db`."""
    legacy = target / "legacy.db"
    shutil.copy2(source_database, legacy)
    for suffix in ("-wal", "-shm"):
        sibling = Path(str(source_database) + suffix)
        if sibling.exists():
            shutil.copy2(sibling, str(legacy) + suffix)
    connection = sqlite3.connect(legacy)
    try:
        connection.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        connection.execute("PRAGMA journal_mode=DELETE")
    finally:
        connection.close()
    for suffix in ("-wal", "-shm"):
        sibling = Path(str(legacy) + suffix)
        if sibling.exists():
            sibling.unlink()
    return legacy


def rows(source, query, parameters=()):
    return source.execute(query, parameters).fetchall()


def insert(connection, table, columns, values):
    placeholders = ", ".join("?" for _ in columns)
    connection.executemany(
        f"INSERT INTO {table} ({', '.join(columns)}) VALUES ({placeholders})", values
    )


def import_lanes(source, target):
    lanes = rows(
        source,
        "SELECT name, key, lastNumber, position FROM lanes ORDER BY position, name",
    )
    for name, key, last_number, _position in lanes:
        if key is None:
            raise ImportError(f"lane {name!r} has no key; its ticket refs cannot be derived")
    insert(
        target,
        "lanes",
        ["name", "key", "last_number", "position", "author"],
        [(name, key, last_number if last_number is not None else 0, position, ACTOR)
         for name, key, last_number, position in lanes],
    )
    return len(lanes)


def import_tickets(source, target):
    tickets = rows(
        source,
        """
        SELECT id, title, description, status, archived, lane, number, origin,
               assignee, link, position, createdAt, completedAt
        FROM tickets ORDER BY rowid
        """,
    )
    insert(
        target,
        "tickets",
        [
            "id", "title", "description", "status", "archived", "lane", "number",
            "origin", "assignee", "link", "position", "created_at", "completed_at",
            "author",
        ],
        [(id, title, description, status, archived, lane, number, origin, assignee,
          link, position, created_at, completed_at, ACTOR)
         for (id, title, description, status, archived, lane, number, origin,
              assignee, link, position, created_at, completed_at) in tickets],
    )
    return len(tickets)


def import_dependencies(source, target):
    edges = rows(
        source,
        "SELECT ticket_id, depends_on_id FROM ticket_dependencies ORDER BY rowid",
    )
    insert(
        target,
        "ticket_dependencies",
        ["ticket_id", "depends_on_id", "author"],
        [(ticket, depends, ACTOR) for ticket, depends in edges],
    )
    return len(edges)


def import_comments(source, target):
    comments = rows(
        source, "SELECT ticketId, author, at, body FROM comments ORDER BY rowid"
    )
    insert(
        target,
        "comments",
        ["ticket_id", "author", "at", "body"],
        comments,
    )
    return len(comments)


def import_sessions(source, target, source_columns, target_columns):
    pairs = [
        ("id", "id"),
        ("project", "project"),
        ("agent", "agent"),
        ("brief", "brief"),
        ("status", "status"),
        ("outcome", "outcome"),
        ("summary", "summary"),
        ("startedAt", "started_at"),
        ("endedAt", "ended_at"),
        ("tokensInput", "tokens_input"),
        ("tokensOutput", "tokens_output"),
        ("tokensReasoning", "tokens_reasoning"),
        ("tokensCacheRead", "tokens_cache_read"),
        ("tokensCacheWrite", "tokens_cache_write"),
        ("seconds", "seconds"),
        ("cost", "cost"),
        ("model", "model"),
        ("tickets", "tickets"),
    ]
    # Additive both ways: the target writes `skills` when its schema defines it,
    # and the source supplies the count map only when its own schema carries it.
    if "skills" in target_columns:
        pairs.append(("skills" if "skills" in source_columns else "NULL", "skills"))
    columns = [source_name for source_name, _ in pairs]
    values = rows(source, f"SELECT {', '.join(columns)} FROM sessions ORDER BY rowid")
    insert(
        target,
        "sessions",
        [target_name for _, target_name in pairs] + ["author"],
        [tuple(value for value in row) + (ACTOR,) for row in values],
    )
    return len(values), "skills" in target_columns and "skills" in source_columns


def import_sprints(source, target):
    sprints = rows(
        source,
        """
        SELECT lane, number, session, mode, batch, runs, close, status, openedAt, closedAt
        FROM sprints ORDER BY lane, number
        """,
    )
    insert(
        target,
        "sprints",
        [
            "lane", "number", "session", "mode", "batch", "runs", "close", "status",
            "opened_at", "closed_at", "author",
        ],
        [tuple(sprint) + (ACTOR,) for sprint in sprints],
    )
    return len(sprints)


def import_settings(source, target):
    settings = rows(source, "SELECT key, value FROM settings ORDER BY rowid")
    insert(
        target,
        "settings",
        ["key", "value", "author"],
        [(key, value, ACTOR) for key, value in settings],
    )
    return len(settings)


def copy_bikeshed(source, target):
    """The vault, byte for byte: scaffold, daily notes and `.raw/` alike."""
    origin = source / "bikeshed"
    destination = target / "bikeshed"
    if not origin.is_dir():
        raise ImportError(f"no bikeshed at {origin}")
    if destination.exists():
        raise ImportError(f"{destination} already exists")
    shutil.copytree(origin, destination, symlinks=True)
    return sum(1 for path in destination.rglob("*") if path.is_file())


def ticket_facts(connection, created_column):
    """Every ticket's ref and the four fields parity item 4 names."""
    keys = {name: key for name, key in rows(connection, "SELECT name, key FROM lanes")}
    facts = {}
    for (identifier, lane, number, status, assignee, title, created_at) in rows(
        connection,
        f"SELECT id, lane, number, status, assignee, title, {created_column} FROM tickets",
    ):
        ref = None
        if lane is not None and number is not None:
            if lane not in keys:
                raise ImportError(f"ticket {identifier} names undeclared lane {lane!r}")
            ref = f"{keys[lane]}-{number}"
        facts[identifier] = (ref, status, assignee, title, created_at)
    return facts


# The usage columns both schemas carry, named as each stores them.
OLD_USAGE = ("cost", "seconds", "tokensInput", "tokensOutput", "tokensReasoning",
             "tokensCacheRead", "tokensCacheWrite")
NEW_USAGE = ("cost", "seconds", "tokens_input", "tokens_output", "tokens_reasoning",
             "tokens_cache_read", "tokens_cache_write")


def usage_totals(connection, columns):
    """The session count and every usage column's total, one row."""
    sums = ", ".join(f"COALESCE(SUM({column}), 0)" for column in columns)
    return connection.execute(f"SELECT COUNT(*), {sums} FROM sessions").fetchone()


def verify(source, target):
    """The acceptance: parity item 4, the record counts and the usage totals."""
    diffs = []
    old = ticket_facts(source, "createdAt")
    new = ticket_facts(target, "created_at")
    for identifier in sorted(set(old) | set(new)):
        if identifier not in new:
            diffs.append(f"ticket {identifier} did not land")
            continue
        if identifier not in old:
            diffs.append(f"ticket {identifier} has no source row")
            continue
        before, after = old[identifier], new[identifier]
        if before != after:
            fields = ("ref", "status", "assignee", "title", "createdAt")
            changes = ", ".join(
                f"{field}: {left!r} -> {right!r}"
                for field, left, right in zip(fields, before, after)
                if left != right
            )
            diffs.append(f"ticket {before[0] or identifier} differs ({changes})")

    old_sessions, old_sprints = (
        rows(source, "SELECT COUNT(*) FROM sessions")[0][0],
        rows(source, "SELECT COUNT(*) FROM sprints")[0][0],
    )
    new_sessions, new_sprints = (
        rows(target, "SELECT COUNT(*) FROM sessions")[0][0],
        rows(target, "SELECT COUNT(*) FROM sprints")[0][0],
    )
    if old_sessions != new_sessions:
        diffs.append(f"session count: {old_sessions} -> {new_sessions}")
    if old_sprints != new_sprints:
        diffs.append(f"sprint count: {old_sprints} -> {new_sprints}")

    old_usage = usage_totals(source, OLD_USAGE)
    new_usage = usage_totals(target, NEW_USAGE)
    names = ("cost", "seconds", "tokens input", "tokens output", "tokens reasoning",
             "tokens cache read", "tokens cache write")
    for name, left, right in zip(names, old_usage[1:], new_usage[1:]):
        if isinstance(left, float) or isinstance(right, float):
            if not math.isclose(left, right, rel_tol=1e-12, abs_tol=1e-9):
                diffs.append(f"usage {name}: {left!r} -> {right!r}")
        elif left != right:
            diffs.append(f"usage {name}: {left!r} -> {right!r}")
    return diffs


def utc_now():
    """The store's timestamp: ISO-8601 UTC, whole seconds."""
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--source", required=True, type=Path,
                        help="the old home: tandem.db and bikeshed/ (read only)")
    parser.add_argument("--target", required=True, type=Path,
                        help="the fresh home to write, absent or empty")
    parser.add_argument("--schema", type=Path,
                        default=Path(__file__).resolve().parent.parent
                        / "rust/core/src/store/schema.rs",
                        help="the store adapter's schema (default: schema.rs)")
    arguments = parser.parse_args(argv)

    source = arguments.source.resolve()
    target = arguments.target.resolve()
    if source == target:
        raise ImportError("--source and --target are the same directory")
    if source in target.parents:
        raise ImportError(f"--target {target} is inside --source {source}")
    if target in source.parents:
        raise ImportError(f"--source {source} is inside --target {target}")
    source_database = source / "tandem.db"
    if not source.is_dir() or not source_database.is_file():
        raise ImportError(f"no tandem.db at {source_database}")
    if target.exists() and any(target.iterdir()):
        raise ImportError(f"--target {target} is not empty")

    version, statements = read_schema(arguments.schema)
    target.mkdir(parents=True, exist_ok=True)
    applied_at = utc_now()
    legacy_path = stage_legacy(source_database, target)
    create_store(target / "tandem.db", version, statements, arguments.schema, applied_at)

    source_db = sqlite3.connect(f"file:{legacy_path}?mode=ro", uri=True)
    target_db = sqlite3.connect(target / "tandem.db")
    try:
        source_sessions = table_columns(source_db, "sessions")
        target_sessions = table_columns(target_db, "sessions")
        with target_db:
            counts = {
                "lanes": import_lanes(source_db, target_db),
                "tickets": import_tickets(source_db, target_db),
                "edges": import_dependencies(source_db, target_db),
                "comments": import_comments(source_db, target_db),
            }
            counts["sessions"], skills_imported = import_sessions(
                source_db, target_db, source_sessions, target_sessions
            )
            counts["sprints"] = import_sprints(source_db, target_db)
            counts["settings"] = import_settings(source_db, target_db)

        files = copy_bikeshed(source, target)
        diffs = verify(source_db, target_db)
    finally:
        source_db.close()
        target_db.close()

    report = {
        "target": str(target),
        "legacy": str(legacy_path),
        "schemaVersion": version,
        "skills": {
            "target": "skills" in target_sessions,
            "source": "skills" in source_sessions,
            "imported": skills_imported,
        },
        "bikeshedFiles": files,
        "rows": counts,
        "verification": {"ok": not diffs, "diffs": diffs},
    }
    print(json.dumps(report, indent=2, sort_keys=True))
    if diffs:
        for diff in diffs:
            print(f"diff: {diff}", file=sys.stderr)
        print(f"verification failed: {len(diffs)} diff(s)", file=sys.stderr)
        return 1
    print("verification: zero diffs", file=sys.stderr)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except (ImportError, sqlite3.Error) as error:
        print(f"import-legacy: {error}", file=sys.stderr)
        sys.exit(2)
