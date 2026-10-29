# terse

One prompt, injected into every session of every harness. Bullets, no filler, technical content intact.

Inspired by [caveman](https://github.com/JuliusBrussee/caveman) — same goal, opposite persona. Caveman drops grammar to save tokens. This drops words and keeps the grammar, because broken English costs accuracy and goodwill.

| | |
|---|---|
| Normal | "The reason your React component is re-rendering is likely because you're creating a new object reference on each render cycle." |
| Terse | "New object ref each render. Inline object prop = new prop identity = re-render. Wrap in `useMemo`." |

## What it is

`TERSE.md` is the whole thing — plain Markdown, no frontmatter, no harness-specific syntax. It is a behaviour the agents have, on by default, and a switch turns it off: at work, then at home.

## Install

Verified paths; `install.sh` does the symlinks and prints the rest.

| Harness | Mechanism | Path |
|---|---|---|
| **OpenCode** | the `terse` plugin appends `TERSE.md` to the system prompt while the switch says on | `~/.config/opencode/plugins/terse` |
| **Claude Code** | user-level rules, loaded every session, all projects | `~/.claude/rules/terse.md` |
| **Codex** | global `AGENTS.md` | `~/.codex/AGENTS.md` |

Claude Code and Codex — symlink, so edits here apply everywhere:

```sh
ln -s "$PWD/TERSE.md" ~/.claude/rules/terse.md
ln -s "$PWD/TERSE.md" ~/.codex/AGENTS.md   # only if you have no global AGENTS.md
```

Codex reads a single global file and has no import syntax. If `~/.codex/AGENTS.md` already exists, paste the body of `TERSE.md` into it under a `## Style` heading instead of replacing it.

These links point into a Git working tree, so this checkout has to stay on a branch that contains it (`main` in this repo). Check out an older branch and the plugin and the switch both dangle: terse goes off silently and `/terse` cannot run. `install.sh` reports a dangling link; the fix is `git -C <this checkout> checkout main`.

## The switch (OpenCode)

`terse` is registered as a plugin, not as an instruction file, because an instruction file is always on and only a config edit turns it off. The plugin appends `TERSE.md` to the system prompt of each request while the switch says on, and appends nothing when it says off — off is the style genuinely absent, not a second instruction arguing with the first.

```sh
/terse          # report
/terse off      # prose from the next request
/terse on
/terse toggle

~/.config/opencode/terse.sh off   # the same switch, from a shell
```

- **Scope**: every agent, every session, including sub-agent children. It is a property of the session's requests, not of one agent's prompt, so it applies whichever agent is selected.
- **Timing**: from the next model request. In flight there is nothing to interrupt.
- **State**: `~/.config/opencode/terse.json`, `{"enabled": true}` or `false`. Missing means on, so a fresh machine behaves as before. Hand-editable; the plugin reads it on every request, so an edit applies without a restart.
- **Storage**: only the agent loop. Title generation and compaction summaries do not pay for the style, since nobody reads them for style.

The `instructions` entry in `~/.config/opencode/opencode.json` must **not** list `TERSE.md` any more: that entry is a second, always-on copy of the same file and would defeat the switch. Remove it after installing the plugin.

Claude Code and Codex keep the instruction-file slot and have no switch — there, delete or restore the symlink.

## Why not a skill or an agent

- **On by default.** A behaviour you have to summon is not a behaviour. A skill loads only when the model decides to call it, which is measurably unreliable.
- **Additive.** The plugin adds text to the harness's own prompt rather than replacing it, so the tool guidance the vendor tuned stays.
- **Not an agent.** A primary agent replaces the base prompt and changes who you are talking to, and the model switch is separate — using one to carry a style makes both harder to reason about. This was tried and dropped.
- **Portable.** Every harness has an instruction slot, and the fallback there is the plain file. Only OpenCode gets the switch, because only OpenCode has a per-request hook.

## Honest math

- **Output only.** This changes nothing about what the agent reads. Input is usually the bigger half of a coding bill.
- **Tool payloads don't shrink.** Diffs, file writes and command output are most of a coding agent's output tokens; a style rule can't touch them.
- **The rule itself costs input.** ~300 tokens on every request, forever — and nothing at all while the switch is off. Cheaper than caveman's ~1,000, but not free.
- **Expect low single digits off the total bill.** The 65–75% figure from caveman's benchmark is prose-only, on prose-heavy prompts.

Cheaper wins first: fewer round trips, read less, don't re-ask.
