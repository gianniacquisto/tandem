#!/usr/bin/env python3
"""One-shot migration: the archived `Sprint <n> — <lane>` tickets become sprint records.

ADR 0032 (ticket 02CA3EC2). **Not a shipped verb** — it runs once per home and is kept
for the record: a verb would be a surface to maintain forever.

What it does:

  1. reads each ticket through `tandem-cli ticket show --id`, so the board is the source;
  2. splits the description into `## The batch` (the ticket ids, in stored order),
     the `## Run` sections (one run entry each, dated by the section's heading) and
     every `## Close` section (their bodies joined — the closes that already read
     "addendum" stay an addendum);
  3. renumbers **per lane by `createdAt`** — the titles lie (three archived tickets
     read `Sprint 1 — Tandem`), the records do not — and gives each record's first
     run entry the provenance;
  4. writes the records into the home's legacy `sprints.json`, then has the product
     import it with a write that changes nothing — `sprint close` aimed at one of the
     records just written (`--number`), which has already closed — and reads them back
     through `sprint list`. The store imports a legacy file inside a domain's write
     transaction, and only there; a read never imports (ADR 0031). `openedAt` = the
     ticket's `createdAt`, `closedAt` = its `completedAt`, mode `serial`, the opening
     session where the ticket names one.

Nothing is deleted: the tickets stay archived. A ticket already carried by a record
(the provenance string is the marker) is skipped, so a second run writes nothing.

Usage:
    TANDEM_CLI=<tandem-cli> Scripts/migrate-sprints.py [ticket-id ...]

The ticket ids default to the five archived sprints; each id is a positional argument
for the test suite and for a re-run that names its corpus. `TANDEM_HOME` is the
product's to resolve — the home is read off `tandem-cli doctor`, not guessed here — so
a throwaway copy is proved by pointing `TANDEM_HOME` at it.
"""

import json
import os
import re
import subprocess
import sys

CLI = os.environ.get("TANDEM_CLI", "tandem-cli")

# The five archived sprint tickets: four in Tandem, one in code-rank.
DEFAULT_TICKETS = [
    "54A38F10-88FF-4D53-90DD-AC50BE097C01",
    "401A1E40-5C26-46EE-8A22-9E0D0A50F98F",
    "3A73BCB6-334F-41D7-BB28-57CF7B0A4781",
    "080A5A50-0C4A-4D58-8711-10BC2194A545",
    "2AE65AF4-ACA8-4446-BDE7-6B7286A0D64A",
]

UUID_RE = re.compile(
    r"[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}"
)
SESSION_RE = re.compile(r"\bses_[A-Za-z0-9]+")
DATE_RE = re.compile(r"\b\d{4}-\d{2}-\d{2}\b")


class MigrationError(Exception):
    pass


def cli(*args):
    process = subprocess.run([CLI, *args], capture_output=True, text=True)
    if process.returncode != 0:
        detail = (process.stderr or process.stdout).strip()
        raise MigrationError(f"tandem-cli {' '.join(args)} failed: {detail}")
    return json.loads(process.stdout)


def home_directory():
    """The home the product resolves, read from `doctor` rather than guessed here."""
    report = cli("doctor")
    for check in report.get("checks", []):
        if check.get("name") == "home":
            detail = check.get("detail", "")
            return detail.rsplit(" (", 1)[0]
    raise MigrationError("tandem-cli doctor reported no home")


def sections(description):
    """[(heading, body)] for every `## ` section, in order; heading without the hashes."""
    found = []
    heading, body = None, []
    for line in description.splitlines():
        if line.startswith("## "):
            if heading is not None:
                found.append((heading, "\n".join(body).strip()))
            heading, body = line[3:].strip(), []
        elif heading is not None:
            body.append(line)
    if heading is not None:
        found.append((heading, "\n".join(body).strip()))
    return found


def batch_of(description):
    """The `## The batch` ticket ids, in stored order, deduped, canonical uppercase.

    Only the section's bullet lines are the batch — the prose around them can name a
    session ticket (a UUID too), and that is not a ticket the sprint carried.
    """
    ids = []
    for heading, body in sections(description):
        if not heading.lower().startswith("the batch"):
            continue
        for line in body.splitlines():
            if not re.match(r"^\s*[-*]\s+", line):
                continue
            for raw in UUID_RE.findall(line):
                uid = raw.upper()
                if uid not in ids:
                    ids.append(uid)
    return ids


def run_entries(description):
    """One entry per non-empty `## Run` section, dated by the section's heading."""
    entries = []
    for heading, body in sections(description):
        if not heading.lower().startswith("run"):
            continue
        if not body or body.strip("_() \t\n").lower() == "none yet":
            continue
        date = DATE_RE.search(heading)
        stamp = f"{date.group(0)}T00:00:00Z" if date else None
        if stamp is None:
            raise MigrationError(f"a `## Run` section names no date: {heading}")
        entries.append(f"{stamp} — {body}")
    return entries


def close_of(description):
    """The `## Close` sections' bodies joined; `None` when the ticket wrote no close."""
    parts = [
        body
        for heading, body in sections(description)
        if heading.lower().startswith("close") and body
    ]
    return "\n\n".join(parts) if parts else None


def session_of(description):
    match = SESSION_RE.search(description)
    return match.group(0) if match else None


def record_for(ticket, number):
    """One record from one ticket. The ticket's own words, its own times."""
    ticket_id = ticket["id"].upper()
    title = ticket["title"]
    description = ticket.get("description") or ""
    opened_at = ticket["createdAt"]
    closed_at = ticket.get("completedAt")
    if not closed_at:
        raise MigrationError(f"{ticket_id} is not completed; a migrated sprint is closed")
    runs = [f"{opened_at} — migrated from ticket {ticket_id} — '{title}'"]
    runs.extend(run_entries(description))
    return {
        "lane": ticket["lane"],
        "number": number,
        "session": session_of(description),
        "mode": "serial",
        "batch": batch_of(description),
        "runs": runs,
        "close": close_of(description),
        "status": "closed",
        "openedAt": opened_at,
        "closedAt": closed_at,
    }


def numbering(tickets):
    """(ticket, lane, number): numbered per lane by `createdAt`, one-based."""
    lanes = {}
    for ticket in tickets:
        lanes.setdefault(ticket["lane"], []).append(ticket)
    placed = []
    for lane, members in lanes.items():
        members.sort(key=lambda t: (t["createdAt"], t["id"]))
        for index, ticket in enumerate(members, start=1):
            placed.append((ticket, lane, index))
    return placed


def main(argv):
    ticket_ids = argv[1:] or DEFAULT_TICKETS
    home = home_directory()
    tickets = [cli("ticket", "show", "--id", ticket_id) for ticket_id in ticket_ids]

    existing = cli("sprint", "list").get("sprints", [])
    carried = {
        run.split("migrated from ticket ", 1)[1].split(" ", 1)[0].upper()
        for record in existing
        for run in record.get("runs", [])
        if "migrated from ticket " in run
    }
    taken = {(record["lane"], record["number"]) for record in existing}

    written, skipped, reasons = [], 0, []
    for ticket, lane, number in numbering(tickets):
        ticket_id = ticket["id"].upper()
        if ticket_id in carried:
            skipped += 1
            reasons.append(f"{ticket_id} — already carried by a record")
            continue
        if (lane, number) in taken:
            skipped += 1
            reasons.append(f"{ticket_id} — Sprint {number} in {lane} is already recorded")
            continue
        written.append(record_for(ticket, number))
        taken.add((lane, number))

    if written:
        path = os.path.join(home, "sprints.json")
        payload = {"version": 1, "sprints": existing + written}
        with open(path, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, indent=2)
            handle.write("\n")
        # The store imports its legacy file inside the *write* transaction, and only
        # there: a read never imports (ADR 0031). So trigger the write without a
        # change — `close` aimed at a record this run just wrote, via `--number`.
        # Every written record is closed, so `close` answers "already closed" and
        # writes nothing, and the import has run by the time it returns. Aimed at
        # the lane (no `--number`) the close resolves the lane's *open* record and
        # stamps the live sprint with a nil note (TAN-192).
        written_record = written[0]
        cli("sprint", "close", "--lane", written_record["lane"],
            "--number", str(written_record["number"]))

        imported = cli("sprint", "list").get("sprints", [])
        by_handle = {(record["lane"], record["number"]): record for record in imported}
        for record in written:
            found = by_handle.get((record["lane"], record["number"]))
            provenance = f"migrated from ticket {next(t['id'].upper() for t, l, n in numbering(tickets) if l == record['lane'] and n == record['number'])}"
            if found is None or not any(provenance in run for run in found.get("runs", [])):
                raise MigrationError(
                    f"Sprint {record['number']} in {record['lane']} did not land in the store; "
                    f"the home's sprints domain may already be imported ({home})")

    for ticket, lane, number in sorted(numbering(tickets), key=lambda row: (row[1], row[2])):
        print(f"{ticket['id'].upper()}  {lane:<12}  Sprint {number}")
    for reason in reasons:
        print(f"skip: {reason}")
    print(f"wrote {len(written)}, skipped {skipped}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except MigrationError as error:
        print(f"migrate-sprints: {error}", file=sys.stderr)
        sys.exit(1)
