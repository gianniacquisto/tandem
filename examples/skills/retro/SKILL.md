---
name: retro
description: "Run a retrospective on a named coding session, on manual invocation only, and file what it finds as backlog tickets in the project the pass ran in. Reach for it when the user asks for a retro or a retrospective, or asks what a pass should have done differently."
---

Derived from the `retro` skill in [mattpocock/skills](https://github.com/mattpocock/skills) (MIT, see `NOTICE.txt`), adapted: the output is tickets on the board.

You find improvements to the coding agent's **environment** (the files, skills, checks and tools it works with), not to the code it wrote. A ticket is the whole output; filing nothing is a valid result. Only the human invokes a retro: no skill or pass ends by running one.

## Read

1. **The session transcript**, the primary source: a finding with no message behind it is a guess. No session named means the most recently updated one that is not this one; say which you took. On opencode: `opencode api session.list` for the id, then `opencode api session.message.list --param sessionID=<id> --param limit=20 --param order=asc`, redirected to a file and read with `jq` (a piped body is silently truncated). Page with `--param cursor=<.cursor.next>` until `jq '.data|length'` is 0, and drop `order` on continuation pages (order with cursor returns nothing). A page that fails to parse means a partial read: re-read it smaller and say so.
2. **The environment**: the repo's `AGENTS.md`, its check command, and `ticket_list` with `filter` all. A finding an open ticket covers is a duplicate.

## The bar

A finding is filed only with all three: an **anchor** (a `file:line`, an exact command, or a message that shows the cost), a **named cost**, and **the owner's next move**. Look for: hard-to-find files, a mistake a check could have caught (a check that exists but is unwired is the finding), a rule the reviewer should enforce, expensive tool calls, instructions that changed nothing, and information the agent never had. More than five findings usually means the bar slipped: keep the clearest and say what you dropped.

## File

The project is derived as the sprint derives it (`skills/tandem-sprint/SKILL.md`, §1) from the directory the session ran in; `Retro` only for a finding no project owns (a project named Retro). Dedupe with `ticket_list` and `text`, then `ticket_add` with `title` "Retro: <one specific sentence>", `status` backlog, `origin` retro, `project`, and a `description` of `Filed by retro (<session id>).` then **Why** (anchor and cost), **What**, and **Done when** where the session shows it. No options, no recommendation, and no edit to the code the session touched.

## Report

```
RETRO: <session id>: <n> filed, <m> skipped: <the finding that matters most>
```

Then one line per ticket filed and per candidate skipped, with its reason.
