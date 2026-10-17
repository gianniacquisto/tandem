# Research: model-invoked skill mechanics (OpenCode + Claude Code)

Resolves [04-model-invocation](../issues/04-model-invocation.md). Primary sources only: OpenCode v2 docs + source (repo `anomalyco/opencode`, dev @ `07bbd0f`, 2026-10-17), Claude Code official docs (repo `anthropics/claude-code` contains no product source — issues, plugins, and changelog only — so its docs are the primary source), and the [Agent Skills spec](https://agentskills.io/specification) both harnesses implement. Local context: this Mac runs OpenCode v2.0.32; `~/.claude/` does not exist yet; `~/.agents/skills/` already holds symlinked skills from a repo (the matt-pocock-skills pattern).

## TL;DR — per-harness recommendation

| | OpenCode v2 | Claude Code |
|---|---|---|
| Global dir | `~/.config/opencode/skills/<id>/SKILL.md`; compat: `~/.claude/skills/`, `~/.agents/skills/` | `~/.claude/skills/<name>/SKILL.md` |
| Per-repo dir | `.opencode/skills/<id>/SKILL.md` (walked from cwd to project root); compat: `.claude/skills/`, `.agents/skills/` likewise | `.claude/skills/<name>/SKILL.md` (start dir + parents to repo root) |
| Extra sources | `skills` array in `opencode.json` (dirs or HTTP catalogs), combined not replaced | `--add-dir` dirs, plugins, managed settings |
| Symlinks | Followed (`symlink: true` in scan; watched at resolved real path) — source-verified | Supported: "a `<skill-name>` entry … can be a symlink to a directory elsewhere … loads the skill once even if several locations point at the same target" |
| File shape | `tandem-memory/SKILL.md` (or flat `tandem-memory.md`); ID = parent dir name, case-sensitive | `tandem-memory/SKILL.md`; command name = frontmatter `name` or dir name |
| Frontmatter → model-invoked | Defaults: just `name` + `description`. Do **not** set `disable-model-invocation` or `metadata.opencode/autoinvoke: "false"` (either makes it user-invocable-only: hidden from the model list, loadable only by exact ID / `@tandem-memory`) | Defaults: just `description`. Do **not** set `disable-model-invocation: true` (removes the description from context entirely → user-invocable-only). `user-invocable: false` is the other axis (hides from human `/name`) and is not needed |
| Where the description sits | Rendered into the session's system context as an `<available_skills>` block (ID, name, description only) at each model step; deltas re-render on change | "In a regular session, skill descriptions are loaded into context so Claude knows what's available, but full skill content only loads when invoked." Listing always contains every name; descriptions fit within a 1%-of-context-window budget (least-invoked dropped first) |
| How firing is decided | Model calls `skill({id})` — tool description: "Load a specialized skill's instructions … when the task at hand matches its description" | "Claude uses this [description] to decide when to apply the skill automatically" |
| Once loaded | Body (no frontmatter) appended to the conversation, with base dir + up to 10 sampled supporting-file paths; stays in context; permission check (`allow`/`ask`/`deny`, last match wins) at load | Rendered `SKILL.md` enters as one message and **stays across turns**; re-invoking with identical content adds a note, not a second copy; compaction re-attaches ≤5k tokens per skill (25k total) |
| Hot reload on repo update | Filesystem watcher, 100 ms debounce → rescan + reload, no restart (source-verified) | Watches skill dirs; changes under `~/.claude/skills/` picked up mid-session, no restart |
| Human invocation | `@tandem-memory` in a prompt | `/tandem-memory` |

## 1. Discovery — verified detail

### OpenCode v2 (source-verified)

- Global roots: `~/.config/opencode` (the global config dir — its `skills/` and `skill/` subdirs are both scanned, per `packages/core/src/config/plugin/skill.ts` `sources()`), plus compatibility roots `~/.claude` and `~/.agents` (per `packages/core/src/config/discovery.ts`: `globalClaudeDirectory`, `globalAgentsDirectory`), each scanned at `<root>/skills` by the compatibility plugin (`packages/core/src/config/plugin/compatibility.ts`).
- Project roots: walks **up** from the working directory (`fs.up` in `discovery.ts`); at each level checks `.claude/`, `.agents/`, `.opencode/` subdirs. Matches the v2 docs: "OpenCode searches from the current directory up to the project root and includes matching directories at every level."
- Config `skills` array (`opencode.json`/`.jsonc`): extra local dirs (relative = from the active working dir, `~/` expanded, or absolute) or `http(s)` catalog URLs. "Every discovered config file contributes its entries. `skills` arrays are combined rather than replaced." (This is the mechanism already in use on this Mac in `~/.config/opencode/opencode.json` for matt-pocock-skills.)
- File shape: scan pattern `{*.md,**/SKILL.md}` — a flat `.md` at a source root, or a file named exactly `SKILL.md` at any depth. **Symlinks are followed** (`symlink: true` in the scan call, both plugins). Skill ID = parent directory name (flat root file: the file's basename), exact and case-sensitive; frontmatter `name` is a display label only.
- Precedence on ID collision — later-registered source wins: built-ins → `.claude/skills` (global, then farthest ancestor → cwd) → `.agents/skills` (same order) → `~/.config/opencode/skills` → project `.opencode/skills` (root → cwd) → explicit `skills` config entries. Dedup is by ID (`loaded.set(skill.id, …)`), so pointing two symlinks at one target loads the skill once.

### Claude Code (docs)

| Location | Path | Loads in |
|---|---|---|
| Personal | `~/.claude/skills/<name>/SKILL.md` | All projects on this machine (not Cowork/cloud) |
| Project | `.claude/skills/<name>/SKILL.md` | Sessions in this repo (start dir + every parent to repo root) |
| Nested | `<subdir>/.claude/skills/<name>/SKILL.md` | Lazy: loads when Claude first touches files under `<subdir>` |
| Additional dir | `.claude/skills/…` in `--add-dir` dirs | That session |
| Plugin | `<plugin>/skills/<name>/SKILL.md` | Wherever the plugin is enabled |
| claude.ai sync | `~/.claude/skills/synced/` | Account-signed-in sessions only |

Name collisions: enterprise > personal > project. Reserved names to avoid: `synced`, `anthropic-skills*`. `tandem-memory` is none of these.

## 2. Frontmatter that makes the skill model-invoked

The Agent Skills spec requires `name` (≤64 chars, `^[a-z0-9]+(-[a-z0-9]+)*$`, must match the directory) and `description` (≤1024 chars, "what the skill does and when to use it … include specific keywords that help agents identify relevant tasks"). OpenCode **v1** enforces both; **v2** accepts all frontmatter as optional (skills without a `description` are simply not advertised). Claude Code makes all fields optional, `description` recommended. Writing `name` + `description` is therefore the portable, version-robust core.

- **OpenCode**: model-invoked is the *default*. The model-facing list contains exactly the permitted skills that have a description and do not set autoinvoke to false (`packages/core/src/skill/instructions.ts`: `skill.description === undefined || skill.autoinvoke === false → excluded`). Two switches make it *user*-invocable-only (the opposite of the goal): `metadata: { "opencode/autoinvoke": "false" }` or top-level `disable-model-invocation: true` (v2 reads the latter as the former — `packages/core/src/config/plugin/skill-file.ts`; test `reads disable-model-invocation as autoinvoke`). `user-invocable`-style isolation does not exist as a flag in v2; a hidden skill is still loadable by exact ID or by the user writing `@tandem-memory`.
- **Claude Code**: model-invoked is the *default* — the docs' table row "(default): You can invoke: Yes | Claude can invoke: Yes | When loaded: **Description always in context**, full skill loads when invoked." `disable-model-invocation: true` flips it to user-only and **removes the description from context**. `user-invocable: false` is the human-side axis (hide from `/` menu) and does not affect model invocation — not needed here; leaving human invocation on is harmless.
- **Cross-harness portability check (tested)**: OpenCode v2's frontmatter decoder is a strict 4-field struct (`name`, `description`, `metadata`, `disable-model-invocation`). Reproducing that struct with the same effect version the source pins (effect 4.0.1) shows unknown keys — `when_to_use`, `user-invocable`, `license`, `compatibility` — are **silently dropped**, not errors: a file with Claude-Code-only fields still parses and loads in OpenCode.

Recommended portable frontmatter:

```yaml
---
name: tandem-memory
description: <scope first, then both trigger branches; ≤1,024 chars>
when_to_use: <optional Claude-Code-only sugar: trigger phrases / example prompts; OpenCode ignores it>
---
```

Do not add `disable-model-invocation`, `metadata.opencode/autoinvoke`, or `user-invocable`. Do not add Claude's `paths` (see §3, rejected lever).

## 3. Invocation semantics — and description-branch wording

**Mechanics, both harnesses:** the description is the *only* always-loaded trigger surface. Neither harness has an event that force-loads a skill; the model reads the description in context and decides, per step, whether the current task/session state matches, then fires the skill tool. So the description must (a) state the scope condition the model can actually detect, (b) name the two trigger moments concretely, and (c) stay within length limits: 1,024 chars hard cap (OpenCode v1; v2 doesn't enforce — keep short anyway) and, for Claude Code, `description` + `when_to_use` is truncated at 1,536 chars per entry, with the whole listing bounded at 1% of the context window (least-invoked descriptions dropped first when over budget). Claude Code's own troubleshooting says it directly: not triggering → "make the description includes keywords users would naturally say"; triggering too often → "make the description more specific."

**Key design consequence:** once the skill fires *once*, its body stays in context across turns (OpenCode appends the body to the conversation; Claude Code: "Claude Code does not re-read the skill file on later turns" — standing instructions, not one-time steps). So branch B (recording as work happens) does not need to re-fire: the description's job is to get the skill loaded early in the session (branch A), and the *body* then carries the "record highlights as you go" discipline for the rest of the session. The description must therefore reliably fire at least once per session.

**Branch A — session-start orientation.** There is no "session start" event visible to the model; what it has at step 1 is the user's first message + the always-loaded skill list. Phrase the trigger in terms the model can detect at that moment:

> … At the start of a session in a \<Tandem-tracked project\> — before reading or editing any project files — load this skill to orient from the bikeshed: find where the work left off and pick up open threads.

**Branch B — recording work highlights.** Name the concrete moments, not the abstract duty (spec: "specific keywords"):

> … As work happens, record a memory note when you make a non-obvious decision (with one-line rationale), discover a gotcha or hard-won fact, finish a significant unit of work, or start/close an open thread. Do not batch highlights to the end of the session.

**Spurious-firing control.** Put the scope clause first so the model prunes non-matching contexts before evaluating the triggers; add an explicit negative ("Do not use in projects that don't use Tandem") if the scope is narrower than "every session on this Mac". The one genuinely open variable is *what marks a session as Tandem-tracked* (the map's destination says sessions "in any repo on this Mac" should see it, but the bikeshed also doubles as the wiki of all work — so "all local sessions" may be the intent; if narrower, the most detectable marker for the model is the Tandem MCP todo tools being present in the session, or the repo being the tandem app repo). That marker is a handoff decision; the wording above slots it in at `\<Tandem-tracked project\>`.

**Levers examined and rejected for this skill:**
- Claude `paths` frontmatter — restricts auto-activation to "working with files matching the patterns"; would suppress branch A (no files touched yet at session start) and the vault lives outside any repo.
- `disable-model-invocation` / `opencode/autoinvoke: false` (either harness) — that is the user-invoked-only configuration; the inverse of the goal.
- Hooks (Claude Code) / no OpenCode equivalent — per-event guarantees are a different mechanism; Claude's own docs push rules that "must hold every time" out of skills into hooks. A hook could complement (e.g., inject a nudge at SessionStart) but the map settled on description-driven model invocation, which is fully supported by both harnesses.

## 4. Install/link — expose `skills/tandem-memory/` globally to both harnesses

**Recommendation: symlink, not copy.** Two symlinks, one source of truth in the repo:

```bash
ln -s /Users/gianniacquisto/projects/tandem/skills/tandem-memory ~/.agents/skills/tandem-memory
mkdir -p ~/.claude/skills
ln -s /Users/gianniacquisto/projects/tandem/skills/tandem-memory ~/.claude/skills/tandem-memory
```

- `~/.agents/skills/tandem-memory` → covers OpenCode (global agent-compatible root; matches the convention already used on this Mac for matt-pocock-skills).
- `~/.claude/skills/tandem-memory` → covers Claude Code (its only personal location; Claude Code does **not** read `~/.agents/skills`). It is also inside OpenCode's search space, but ID-dedup means the skill loads once, and Claude Code explicitly loads a skill "once even if several locations point at the same target."
- Why symlink beats copy: the map's settled decision is a single source of truth in the repo with no per-repo copies, and both harnesses follow symlinks (OpenCode: `symlink: true` scan + realPath resolution, source-verified; Claude Code: documented). Copies would fork the source of truth and go stale.
- **What happens when the repo is updated:** a commit/pull rewrites the files in place under the same path, so the symlinks keep pointing at live content — no re-install step. Both harnesses hot-reload: OpenCode's watcher (100 ms debounce) rescans and re-renders the skill list in a running session; Claude Code's live-change-detection picks up edits under `~/.claude/skills/` mid-session. Edge cases to know: (1) checking out a branch that removes `skills/tandem-memory/` breaks loading in both harnesses until restored; (2) re-cloning the repo to a different path invalidates the absolute symlink targets — re-run the two `ln -s` commands; (3) content seen mid-session by an already-running agent is whatever was loaded at fire time — both harnesses re-load on the next invocation.
- Alternatives considered: OpenCode's `opencode.json` `skills` array (works, already used here, combined not replaced) — rejected as the *primary* mechanism because Claude Code has no equivalent; a single `~/.agents/skills` symlink only — rejected because it misses Claude Code.
- Name check: `tandem-memory` is lowercase kebab-case, ≤64 chars, matches its directory, no `--`, and no collision with reserved names or bundled commands (Claude Code: a local skill named `tandem-memory` shadows nothing; there is no bundled skill by that name).

## Verification notes

- OpenCode claims verified against source at `anomalyco/opencode` dev @ `07bbd0f` (2026-10-17): `packages/core/src/skill/instructions.ts` (model-facing list + rendering), `packages/core/src/skill.ts` (permission filter, load output shape), `packages/core/src/skill/discovery.ts` (HTTP catalogs), `packages/core/src/config/plugin/skill.ts` (config `skills` array, `skill`/`skills` dirs, watcher, 100 ms debounce), `packages/core/src/config/plugin/compatibility.ts` (`~/.claude`, `~/.agents` roots, `{*.md,**/SKILL.md}` + `symlink: true`), `packages/core/src/config/plugin/skill-file.ts` (frontmatter struct, `disable-model-invocation` → autoinvoke), `packages/core/src/config/discovery.ts` (global/project root discovery, upward walk), `packages/core/src/tool/plugin/skill.ts` (tool name/description/input).
- Frontmatter extra-field behavior tested locally: effect@4.0.1 (the version the source pins), identical 4-field struct, `decodeUnknownResult` — `when_to_use`, `user-invocable`, `license`, `compatibility` all silently dropped; valid `metadata` maps preserved.
- Claude Code: official docs are the primary source (the GitHub repo carries no product source). Doc page: [Extend Claude with skills](https://code.claude.com/docs/en/skills) (fetched 2026-10-17; the "Frontmatter reference", "Control who invokes a skill", "Skill content lifecycle", "Where skills live", "Skill descriptions are cut short", and "Troubleshooting" sections).
- Local environment: `opencode v2.0.32`; `~/.config/opencode/opencode.json` already uses the `skills` array for matt-pocock-skills; `~/.agents/skills/` populated with repo symlinks; `~/.claude/` absent (the install step creates `~/.claude/skills/`).

## Sources

- OpenCode v2 docs — Agent Skills: https://opencode.ai/v2/docs/skills/ (Discovery, Sources, Catalogs, Frontmatter, IDs, Precedence, Loading, Permissions)
- OpenCode v1 docs — Agent Skills (version-drift reference; `name`/`description` required, 1,024-char cap): https://opencode.ai/docs/skills/
- OpenCode source: https://github.com/anomalyco/opencode (files listed in Verification notes)
- Claude Code docs — Extend Claude with skills: https://code.claude.com/docs/en/skills
- Claude Code repo (issues/plugins/changelog; no product source): https://github.com/anthropics/claude-code
- Agent Skills spec: https://agentskills.io/specification
