---
name: retro
description: "Run a retrospective on a coding session and file what it finds as backlog tickets in the Retro lane. Runs at every sprint close — the sprint dispatches it on the sprint session's own transcript — and still invocable alone: reach for it when the user asks for a retro or a retrospective, or asks what a session should have done differently."
---

Derived from the `retro` skill in [mattpocock/skills](https://github.com/mattpocock/skills) (MIT, see `NOTICE.txt`), adapted: the output is tickets on the board, not a report to the user.

You suggest improvements to the coding agent's **environment** — the files, skills, checks and tools it works with — not to the code it wrote. You produce no code and no design; a ticket is the whole output. A retro that files nothing is a valid result; one that files a ticket nobody can act on is not.

## Read

1. Load `writing-for-agents` for the writing style.
2. **The session** — `opencode api session.list` finds the id; `opencode api session.message.list --param sessionID=<id> --param limit=20 --param order=asc` returns the transcript, tool calls included, oldest first, and it pages by cursor: re-call with `--param cursor=<cursor.next from the last response>` until a page comes back empty. The envelope is `{"cursor": {...}, "data": [...]}`: the page is `.data`, the next call's cursor is `.cursor.next`, and the empty-page probe is `jq '.data|length'`, never `.messages|length`, which reads 0 on a full page. `--param order=asc` goes on the first call only. No offset parameter exists — cursor paging is the only way past the first page, so a single call returns the newest slice only. Two read rules: do not pass `order` on continuation pages (order + cursor together returns zero messages, silently); redirect each response to a file and `jq` the file — the CLI byte-truncates a large body when stdout is a pipe (observed cuts at 262144–393216 on a 516 KB body, mid-string, silently). The transcript is the primary source: a finding without a message in it is a guess. No session named → the most recently updated session that is not this one, and say which you took. A read that comes back cut anyway — `jq: parse error: Unfinished string at EOF`, or a page that fails to parse — means the view is partial: say so, and re-read the missing pages smaller, rather than presenting what you read as the whole transcript.
3. **The environment** — the repo's `AGENTS.md`, its check command, and the board (`tandem-cli ticket list --filter all`). A finding an open ticket already covers is a duplicate, not a finding.

## Categories

- **Navigation** — hard-to-find files, hidden dependencies, a missing pointer.
- **Automated checks** — a mistake a check could have caught. Read the repo's own check command first: a check that exists but sits unwired is the finding, and a repo with no guardrail at all is itself a finding.
- **Coding standards** — a rule for the reviewer to enforce; a mechanical violation deserves a deterministic check over a written rule.
- **Tool economy** — expensive tool calls that could be streamlined.
- **No-ops** — steering instructions that change nothing.
- **Information access** — information the agent never had.

## The bar

A finding becomes a ticket only with all three: an **anchor** (a `file:line`, an exact command, or a message that shows the cost), a **named cost**, and an **owner's next move**. Not filed: generic advice, a preference with no cost, a restatement of an existing rule, a duplicate, your own run. More than five tickets usually means the bar was lowered — pick the clearest costs and say what you dropped.

## File

```sh
tandem-cli ticket list --filter all --text "<title words>"   # dedupe first
tandem-cli lane add --name Retro                              # only when filing and the lane is missing
printf '%s' "$body" | tandem-cli ticket add --title "Retro: <one sentence, specific enough to disagree with>" \
  --status backlog --origin retro --lane Retro --description -
```

`--origin retro`, never assigned. The body is a **capture**, not a brief: `Filed by retro (<session id>).`, then **Why** (the anchor and the cost), **What**, **Done when** (only if the session makes it visible). No options, no recommendation. Never edit the code the session touched — a retro that fixes what it finds cannot report honestly on it.

## Report

```
RETRO: <session id> — <n> filed, <m> skipped — <the finding that matters most>
```

Then the ticket list, one line each, and each skipped candidate with its reason. No questions to the caller, no offer to fix.

The retro's record is its **session record**: end it with the `RETRO:` line as the summary. File a capture only for a finding that is not already a ticket in the log above and whose re-derivation would cost real effort.
