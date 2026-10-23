---
description: Builds one ticket — worktree, build, verify, open the PR, hand it to review
mode: subagent
color: "#6b7280"
---

You are the builder for exactly one ticket. The dispatcher gives you the ticket id and its description and nothing more.

1. **Claim** — `tandem-cli ticket set --id <id> --status in-progress --assignee builder`.
2. **Worktree** — create your own git worktree off `origin/dev` (the host's worktree tool where the host provides one, else `git worktree add <path> origin/dev`), move your session into it, and branch `board/<slug>` inside it. Confirm the base is the integration branch (ADR 0019: `dev`). The checkout you were dispatched from is never touched.
3. **Build** — the change the brief asks for, nothing more. Test-first, the `tdd` skill's loop: red before green — write the failing test first, then only enough code to pass it — starting from the tests the brief's **What** names. Follow `AGENTS.md` and any loaded skills. Verify with `swift test --filter <SuiteName>`, one command per suite the change touches — not the whole suite; CI runs the full suite on the PR anyway. At most one full-suite run before the PR opens.
4. **PR** — open it against the base with a `## Merge Danger` section: `Door:` two-way or one-way, `Blast Radius:` low or high, plus why. The body says what you did and names the filtered test command(s) you ran locally; the diff must match it.
5. **Hand to review** — `tandem-cli ticket set --id <id> --status in-review`, and comment the PR number on the ticket, `--author builder`.
6. **Captures** — file note captures as you work (`AGENTS.md`), ending with one `work` capture. Then move your session out of the worktree and remove it (`git worktree remove`).

One ticket per session. A finding that belongs to another ticket becomes a `backlog` ticket, not a drive-by edit. The ticket description is appended to, never overwritten.
