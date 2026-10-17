---
name: setup-tandem-memory
description: Link the tandem-memory skill into OpenCode and Claude Code globally. Run once per machine.
disable-model-invocation: true
---

# Set up Tandem memory

Link this repo's two skills — `tandem-memory` and `setup-tandem-memory` — into both harnesses' global skill roots, so every later session in any repo on this Mac finds the memory skill on its own. The repo stays the single source of truth: symlinks, not copies, and both harnesses hot-reload a repo change with no reinstall.

Two roots cover both harnesses (research: [`model-invocation-mechanics.md` §4](../../.scratch/tandem-memory/research/model-invocation-mechanics.md)):

- `~/.agents/skills/` — OpenCode, and this Mac's existing convention.
- `~/.claude/skills/` — Claude Code; OpenCode reads it too, so the second link reaches Claude Code and costs OpenCode nothing (it dedups by ID).

This is a prompt-driven run-once skill, not a deterministic script: detect the machine's state, present what you found, confirm with the human, then link and verify. Re-running is safe.

## 1. Detect

Report the current state; assume nothing:

- Do the roots exist? `ls -ld ~/.agents/skills ~/.claude/skills 2>/dev/null`
- For each root × each skill (`tandem-memory`, `setup-tandem-memory`), `readlink <root>/<skill>`: missing, correct (resolves to this repo's `skills/<skill>`), or pointing somewhere else?
- Does this repo hold `skills/tandem-memory/` and `skills/setup-tandem-memory/`? Their presence is what the link targets depend on.

Done when you can state, per root, which of the two links is missing, correct, or stale.

## 2. Present and confirm

Summarise the findings in one line per root, then the planned change: create any missing root, and `ln -sfn` both skills into both roots. Lead with the recommendation so the human can accept in a word.

One branch needs an explicit answer: a link that already points at a **different** skill. `ln -sfn` would clobber it, so name it and get agreement before overwriting; otherwise confirm the whole plan once and proceed.

Done when the human has approved the change set (or declined — then stop).

## 3. Link

Run from the tandem repo, so the links point at this checkout:

```bash
repo="$(git rev-parse --show-toplevel)"
mkdir -p ~/.agents/skills ~/.claude/skills
for root in ~/.agents/skills ~/.claude/skills; do
  ln -sfn "$repo/skills/tandem-memory" "$root/tandem-memory"
  ln -sfn "$repo/skills/setup-tandem-memory" "$root/setup-tandem-memory"
done
```

`ln -sfn` replaces any existing link in place, so re-running never nests or errors.

Done when all four links exist.

## 4. Verify

Confirm each link resolves to a real file in this repo, in both roots:

```bash
for root in ~/.agents/skills ~/.claude/skills; do
  for s in tandem-memory setup-tandem-memory; do
    test -f "$root/$s/SKILL.md" && echo "ok      $root/$s" || echo "MISSING $root/$s"
  done
done
```

All four lines read `ok`. Both harnesses follow symlinks, so a resolving path is a resolving skill: `tandem-memory` then appears in OpenCode's model skill list and Claude Code's `/` menu, and a later repo edit hot-reloads into a live session. Tell the human which session finds which skill.

Done when every link resolves to this repo's `skills/<name>/SKILL.md`.

## Undo

Remove the links; the repo is untouched:

```bash
for root in ~/.agents/skills ~/.claude/skills; do
  rm -f "$root/tandem-memory" "$root/setup-tandem-memory"
done
```

## Bootstrap — no links yet

On a machine where neither skill is globally reachable yet, run these two `ln -s` commands from the tandem repo root to make the first links by hand. Each links both skills into one root; after it, `setup-tandem-memory` fires by name from anywhere.

```bash
mkdir -p ~/.agents/skills ~/.claude/skills
ln -s "$PWD/skills/tandem-memory" "$PWD/skills/setup-tandem-memory" ~/.agents/skills/
ln -s "$PWD/skills/tandem-memory" "$PWD/skills/setup-tandem-memory" ~/.claude/skills/
```
