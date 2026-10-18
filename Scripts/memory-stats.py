#!/usr/bin/env python3
"""memory-stats — measure tandem-memory *Record* activity, not skill-tool loads.

The tandem-memory skill has two phases:

  Orient  loads the skill once per session. This is the ONLY thing a skill-tool
          count (or `opencode stats`) can see.
  Record  writes notes to the bikeshed and mutates threads as work happens. It
          never reloads the skill, so it is invisible to a skill-load count.

So "how many times was the skill loaded" measures orientation, not use. This
script counts Record directly from each session's messages and splits every
count by root session vs sub-agent (a session with a parent).

Metrics per session:
  orient   `skill` tool calls whose id is `tandem-memory`   (once per session)
  reads    read/glob/grep calls naming the bikeshed          (orientation effort)
  curated  write/edit calls into a curated bikeshed note      (Index/anchor/topic)
  raw      writes into the hidden `.raw/` capture layer      (write/edit, or `note capture`)
  todos    `tandem-cli todo add|complete` shell calls          (threads)

Record = curated + todos. `raw` is reported separately: it is the capture layer
this measurement exists to justify.

Usage:
    python3 Scripts/memory-stats.py

Requires the `opencode` CLI on PATH (it reads sessions through `opencode api`;
`opencode api` truncates on a pipe, so responses go through a temp file).
"""
import json
import os
import re
import subprocess
import tempfile
from collections import Counter, defaultdict

TMP = os.path.join(tempfile.gettempdir(), "memory-stats-api.json")
# A bikeshed under any Tandem home: ~/.tandem/, ~/.tandem-dev/, a TANDEM_HOME.
BIKESHED = re.compile(r"/\.tandem[^/]*/bikeshed/")


def api(path):
    with open(TMP, "wb") as fh:
        out = subprocess.run(["opencode", "api", "get", path],
                             stdout=fh, stderr=subprocess.PIPE)
    if out.returncode != 0:
        raise SystemExit(f"opencode api failed for {path}\n"
                         f"{out.stderr.decode(errors='replace')}")
    with open(TMP) as fh:
        return json.load(fh)


def paginate(path):
    """Page an `opencode api` list endpoint via its `cursor.next`."""
    items, cursor = [], None
    while True:
        page = api(path + (f"&cursor={cursor}" if cursor else ""))
        items += page["data"]
        cursor = (page.get("cursor") or {}).get("next")
        if not cursor:
            return items


def classify_path(path):
    """`curated` / `raw` / None for a file path in a bikeshed tree.

    Both the real (`~/.tandem`) and dev (`~/.tandem-dev`) homes count. The
    capture layer is `projects/<project>/.raw/...` — a hidden component
    anywhere below the bikeshed root, archive (`.done/`) included."""
    if not path or not BIKESHED.search(path):
        return None
    return "raw" if "/.raw/" in path else "curated"


def classify_shell(cmd):
    """Shell files raw capture notes (`note capture`) and mutates threads
    (`todo add|complete`). A shell `>` heuristic false-positives on `2>&1`, so
    file writes are read from the write/edit tools instead."""
    hits = Counter()
    if "tandem-cli todo add" in cmd or "tandem-cli todo complete" in cmd:
        hits["todos"] += 1
    if "note capture" in cmd:
        hits["raw"] += 1
    return hits


def main():
    sessions = paginate("/api/session?limit=100")
    roots = {s["id"] for s in paginate("/api/session?limit=200&parentID=null")}

    per = defaultdict(lambda: Counter())
    titles = {}
    for s in sessions:
        sid = s["id"]
        titles[sid] = (s.get("title") or "").strip()
        cur = None
        while True:
            url = f"/api/session/{sid}/message?limit=100"
            if cur:
                url += f"&cursor={cur}"
            page = api(url)
            for m in page["data"]:
                if m.get("type") != "assistant":
                    continue
                for p in m.get("content", []):
                    if p.get("type") != "tool":
                        continue
                    name = p.get("name")
                    inp = p.get("state", {}).get("input") or {}
                    blob = json.dumps(inp)
                    if name == "skill" and inp.get("id") == "tandem-memory":
                        per[sid]["orient"] += 1
                    elif name in ("write", "edit"):
                        kind = classify_path(inp.get("path") or inp.get("filePath") or "")
                        if kind:
                            per[sid][kind] += 1
                    elif name in ("read", "glob", "grep"):
                        if "bikeshed" in blob and ".tandem" in blob:
                            per[sid]["reads"] += 1
                    elif name == "shell":
                        per[sid].update(classify_shell(inp.get("command", "")))
            cur = (page.get("cursor") or {}).get("next")
            if not cur:
                break

    all_ids = [s["id"] for s in sessions]
    root_ids = [i for i in all_ids if i in roots]
    sub_ids = [i for i in all_ids if i not in roots]

    def summarize(ids, label):
        rows = [per[i] for i in ids]
        oriented = sum(1 for r in rows if r["orient"])
        curated = sum(r["curated"] for r in rows)
        todos = sum(r["todos"] for r in rows)
        raw = sum(r["raw"] for r in rows)
        recording = sum(1 for r in rows if r["curated"] or r["todos"] or r["raw"])
        print(f"{label:<12} sessions={len(ids):>4}  oriented={oriented:>4}  "
              f"recording={recording:>4}  "
              f"record={curated + todos:>4} (curated={curated}, todos={todos})  "
              f"raw={raw:>4}")

    print(f"\nsessions scanned: {len(all_ids)}\n")
    summarize(all_ids, "all")
    summarize(root_ids, "root/main")
    summarize(sub_ids, "subagent")

    def dist(ids):
        c = Counter()
        for i in ids:
            n = per[i]["curated"] + per[i]["todos"]
            c[0 if n == 0 else 1 if n == 1 else 2 if n <= 3 else 4] += 1
        return c

    d = dist(all_ids)
    print("\nrecords-per-session distribution (all sessions):")
    for k, label in [(0, "0 records"), (1, "1 record "), (2, "2-3     "), (4, "4+      ")]:
        print(f"  {label}: {d[k]}")

    print("\nrecording sessions (orient/reads/curated/raw/todos):")
    print(f"{'':>7}{'reads':>7}{'curated':>9}{'raw':>6}{'todos':>7}  title")
    for sid in sorted(all_ids,
                      key=lambda i: -(per[i]["curated"] + per[i]["todos"] + per[i]["raw"])):
        r = per[sid]
        if not (r["curated"] or r["todos"] or r["raw"]):
            continue
        print(f"{r['orient']:>7}{r['reads']:>7}{r['curated']:>9}{r['raw']:>6}"
              f"{r['todos']:>7}  {titles[sid][:44]}")


if __name__ == "__main__":
    main()
