# Agent, skill and lens: a specialist reader owns each lens, and the gate stays one

The lens set is one reviewer's checklist (`skills/review-lenses/`): it names what to look for, but no one owns the architecture or security intent from the brief through to the PR. The refiner records the intent and the reviewer re-derives it; the specialist who could carry it has no identity to speak with. This record gives each lens a named reader at both ends without splitting the merge gate: the specialist contributes constraints at refine and a conformance read at review, and the reviewer remains the one verdict.

Decided 2026-11-08 (TAN-326). The intended number was 0042; 0042 and 0043 landed while this ticket waited, so this branch takes the next free number, 0044, under the repo's collision rule.

## The three primitives

- **Agent**: a prompt with a model and a session. The four pass owners are `builder`, `refiner`, `reviewer` and `consolidator`; the three specialist readers are `architect`, `security` and `ux`. A specialist starts its own session record with `--agent <name>`, links the ticket, and signs its comments with that name.
- **Skill**: a procedure the session's agent runs. The specialist is not a skill, and the lens is not a skill; `skills/review-lenses/` remains the one lens table.
- **Lens**: a checklist in `skills/review-lenses/`, owned by one reader. The owner is the specialist when one is dispatched, otherwise the reviewer. `code` is always the reviewer's own read and never has a specialist.

## Decision

- **The lens table gains an owner column.** `code` and `ui-design` belong to the `reviewer`; `security` to the `security` reader; `architecture` to the `architect`; `ux` to the `ux` reader. The table stays the single source for the lens, its file and its owner; the lens files keep their three headings and are unchanged.
- **The floor is `code`, `security` and `architecture`.** Every brief names the floor, one clause each on why, plus the surface lenses. The floor is stated once in `skills/review-lenses/SKILL.md`; `agents/refiner.md` and `agents/reviewer.md` point at it and do not restate the list.
- **Dispatch is conditional on a target.** A specialist is dispatched only when its lens has a target in the change. A named lens with no target says nothing, and the brief records that. A specialist that cannot be dispatched is named in the brief and the pass owner applies the lens; never a silent skip.
- **Refine dispatches the specialist as a child.** The refiner chooses the lens set and its owners, then dispatches each specialist whose lens has a target with the ticket id and the lens. The specialist posts a ticket comment signed with its name: the constraints, the ADR obligations, the questions the plan must answer, and the files the change touches. The refiner folds the load-bearing constraints into `## What`, names the owner and the one-clause reason in `## Lenses`, and leaves the detailed read in the comment log. The specialist writes no design and no brief prose; the human round stays the refiner's.
- **Review dispatches the specialist for its conformance read.** The reviewer reads the ticket's `## Lenses` and the specialist comments, then dispatches each named specialist whose lens has a target with the ticket id, the PR, the lens and its own refine comment. The specialist posts a `gh pr comment`, never a review: the gate's review state stays one identity. The comment is the conformance read, `file:line`, with its `## Blocks on` findings. Before the verdict the reviewer records one line per finding, adopted, rebutted with the reason, or not applicable. A `## Blocks on` finding is binding; a merge over one without a recorded reason is itself a finding.
- **The gate stays one.** The reviewer keeps `code` as its own read, applies any lens without a specialist, and keeps the verdict, the merge, the escalation and the door class. A specialist never merges, never pushes, never sets `in-review` or `human`, and its comment contains no verdict.
- **The roster names the specialists beside the pass owners.** `skills/tandem-team/SKILL.md` gains a specialist table listing `architect`, `security` and `ux`, the lens each owns, and the rule that a specialist speaks as `--author`, owns no ticket and never merges. The pass table keeps the four pass owners. A pass owner can be an assignee; a specialist only speaks.
- **The specialist prompts are flat files.** `agents/architect.md`, `agents/security.md` and `agents/ux.md` are flat because the installer's `prompts()` scans `agents/*.md` and the host's symlinks are flat. Each has `description`, `mode: subagent`, a distinct `color`, and no `model` (ADR 0026's amendment: no agent pins a model). Each denies `edit`, `question` and `subagent` in the consolidator's shape, so it reads and comments but cannot write files, ask the human or spawn another reader.
- **The prose names the specialists.** `GLOSSARY.md` defines `Specialist` and updates `Review lens` and `Reviewer`; `README.md` and `skills/tandem-setup/SKILL.md` carry the pass prompts plus the specialists and no stale "four prompts" count. The installed face needs no code change: `prompts()` already copies every `agents/*.md`, and `rust/core/tests/setup_install.rs`'s fixture stays green.
- **The explicit non-changes.** `rust/core/src/domain/settings.rs` is untouched: the assignee picker's default roster stays `human`, `builder`, `refiner`, `reviewer`, `consolidator`, `sprint`. No store, CLI, workflow, runner or schema change lands. A specialist is never an assignee, so no existing home sees a new picker name.
- **The seam is the repo-root prose pin suite.** `rust/core/src/tests/scaffold_convention.rs` reads the live files, not a fixture, because the executable half of this change is the contract's prose. The `lens_rows` helper gains the owner column, and new pins hold the owners, the floor's one home, the specialist comments, the reviewer's finding lines, the roster and the prompts' permissions. The ADR itself is not pinned.

## Why

- **The intent needs a reader at both ends.** The refiner writes what the change must respect and the reviewer re-derives it from the diff. A specialist who contributed the constraint can read the finished work against its own plan, and the comment log shows the human where the constraint came from.
- **The gate stays one.** A second reviewer per specialism would make every ticket's domain its gate and let a specialist review the plan it wrote with no independent read. The reviewer's `code` lens and the ADR obligations remain the independent half.
- **The cost is paid only where the surface earns it.** A prose-only change gets the required lenses and no specialist dispatch. A UX change gets the `ux` reader because the surface has a target, not because a ceremony demands one.

## Decided against

- **The specialist as a pass owner**: the architect is not the refiner and reviewer for its domain; the refiner and reviewer stay the owners, and the specialist contributes.
- **A second reviewer, a second verdict or a second merge authority**: the specialist's `## Blocks on` findings bind, but the reviewer records the resolution and keeps the verdict.
- **The specialist as a lens only**: no new agent, but no named continuity and no second reader; the human chose the agent as a reader.
- **A dedicated `## Specialists` brief section**: the comment log is the detailed read, the brief carries the load-bearing constraints; the brief shape stays.
- **Always dispatching the required specialists**: the floor is a lens floor; dispatch is conditional on a target, so a prose-only change pays no extra agent.
- **Specialists as assignees**: no `--assignee architect`, no default roster change; the picker stays the six pass roles.
- **A `gh pr review` from a specialist**: the gate's review state stays one identity; the specialist posts a PR comment.
- **A model pin on the specialist prompts**: no agent pins a model (ADR 0026's amendment).
- **An `agents/specialists/` subfolder**: flat files keep the installer's `agents/*.md` scan and the host's symlinks working.
- **An executable model change**: no store, CLI, workflow, runner or schema change; this is prose, pins and this ADR.

## Consequences

- A specialist is a new reader with ticket and PR access; its prompt's denials are the guardrail, and its comments carry no secrets.
- The host's agent list grows from four prompts to seven after the next server start. `tandem-cli setup install` copies them with no code change, and `opencode api agent.list` reports what the server read.
- The lens table's owner column is the mapping a later prompt rename must keep in step; the pins fail on an owner with no prompt.
- [ADR 0026](0026-agent-prompts-and-portable-setup.md) is amended in reading: its "agent prompts are the passes" describes the four pass owners, and a specialist is a reader rather than a pass. [ADR 0029](0029-the-ceremony-set.md)'s "no new agent per ceremony" still holds: a specialist is not a ceremony. [ADR 0034](0034-agent-face-in-the-artifact.md) carries the installed face unchanged.
