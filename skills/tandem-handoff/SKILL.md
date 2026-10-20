---
name: tandem-handoff
description: "Assemble a paste-ready handoff block for a new agent session — the open Tandem tickets (`tandem-cli todo list --filter open`), the bikeshed `Index.md`, the matching project note's `Where left off` and Threads, and every open ticket assigned to `human`. Reach for it when the user asks for a handoff, wants to hand this work to a fresh session, asks 'where did we leave off', or wants the current open work gathered into one block to paste into a chat window. Read-only: it edits no note and changes no ticket. Takes an optional project name."
---

# Tandem handoff

A **handoff** is the orient a session does on its own — the `tandem-memory` skill's walk — assembled into one self-contained block a human can paste into a new agent session. It exists because a session asked to orient will do it, but a human pasting text into a chat window cannot rely on that: the block has to carry the state itself.

It is a **read-only concatenation**. It reads `~/.tandem/bikeshed/` and calls `tandem-cli` read verbs; it writes no note and changes no ticket. It is the same shape as the memory protocol's orient (Index → the project's `Where left off` → the open threads) and names its vocabulary after it: **bikeshed**, **curated** note, **Threads**.

## What the block carries

In this order, because a reader meets the map before the detail:

1. the bikeshed **`Index.md`** — pointers and one-line status;
2. the project's **curated note(s)** — `Where left off` and `Threads`;
3. the **open tickets** — `tandem-cli todo list --filter open`;
4. the **open tickets assigned to `human`** — the decisions waiting on a person, called out so the receiving session meets them first.

## The commands

Set the two fixed paths first; they are stable across upgrades (the bikeshed root is `~/.tandem/bikeshed`, the CLI lives in the app bundle).

```sh
BIKESHED="$HOME/.tandem/bikeshed"
CLI="/Applications/Tandem.app/Contents/MacOS/tandem-cli"
```

**1 — the Index.** The Hub; one line per curated note.

```sh
cat "$BIKESHED/Index.md"
```

**2 — the project's `Where left off` and Threads.** A project is a folder (`projects/<project>/`) that can hold several curated notes, so the block prints every note under it, labelled by note name — the ticket's "the matching note" is a folder, not one anchor file. Extract a section by heading range so the body stops at the next `##`:

```sh
show_section() { awk -v h="$1" '$0 == "## " h { f=1; print; next } /^## / { f=0 } f' "$2"; }

for note in "$BIKESHED"/projects/<project>/*.md; do
  printf '\n### %s\n\n' "$(basename "$note" .md)"
  show_section "Where left off" "$note"
  show_section "Threads" "$note"
done
```

A note whose section is missing or empty prints its heading and nothing under it; say so in the block rather than dropping the note — a missing `Where left off` is itself a signal. A `<project>` folder with no curated note at all (only a saddlebag) prints nothing from step 2; say that too, so the reader knows the project has no memory note yet rather than a truncated block.

**3 — the open tickets, all of them.** One JSON object in the list's stored order; a human pasting the block wants the raw shape, not a reformat.

```sh
"$CLI" todo list --filter open
```

**4 — the open tickets assigned to `human`.** The subset waiting on a person — the decisions a new session should not start without.

```sh
"$CLI" todo list --filter open --assignee human
```

Tickets assigned to `human` also appear in step 3's full list; call them out first and keep the full list for context.

## With no project name

Omit the project argument and the block is the **Index plus each project's one-line status** — the light version for "what is open on this machine", not "resume this project". The Index's `## Projects` section already carries one line per curated note; to re-derive the same line straight from each note rather than trust the Index, take the first non-empty line under `Where left off`:

```sh
one_line() { awk '/^## Where left off/{f=1;next} f&&/^## /{exit} f&&NF{print;exit}' "$1"; }
setopt nullglob 2>/dev/null || shopt -s nullglob

for project in "$BIKESHED"/projects/*/; do
  notes=("$project"*.md)
  ((${#notes[@]})) || continue
  printf '\n%s\n' "$(basename "$project")"
  for note in "${notes[@]}"; do
    printf '  %s — %s\n' "$(basename "$note" .md)" "$(one_line "$note")"
  done
done
```

The `nullglob` line (`setopt` on zsh, `shopt` on bash) and the `notes` guard skip a folder with no curated notes — a project that exists only for its saddlebag — instead of failing the glob or printing a bare name. Keep them: `projects/fleet/` is exactly that folder on this machine today.

Then the two ticket commands (steps 3 and 4) follow. Skip step 2's full sections: without a project there is no "the project's" note, and the per-project one-liners are the point.

## Read-only

This skill reads and prints. It **edits no note and changes no ticket** — no `note rename`, no `todo set`, `move`, `complete`, or `archive`, and no write to a curated note or the Index (the `tandem-memory` rule: those have one writer, the consolidation pass). A handoff that wants to record something files a `capture` instead, or leaves it to the receiving session.

## Done when

Running it on this machine prints the block with the current open tickets. The block is **self-contained**: a fresh session can name the project state and the next move from it alone — current state, next move, open threads in hand — without reading the repo first. And no curated note is edited.
