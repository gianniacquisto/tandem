# AGENTS.md

## Work record

The ticket comment log is the work record: a pass narrates what landed, the outcome and next status, and any issues on every ticket it carried, and that comment is the only record kept. No pass record, usage report or capture layer sits beside it (ADR 0045).

**Agents call Tandem through the MCP tools**, never through the shell: `tandem-cli` is the human's and the scripts' door, documented in `examples/skills/tandem/SKILL.md`.

## Prose

No em dash (U+2014) in anything an agent writes: use a comma, a colon, a full stop, or a hyphen instead. The ban covers PR bodies, review replies and issue comments; ticket titles, descriptions and comments; bikeshed notes and captures; commit messages, code comments and docs. Before filing a comment or opening a PR, run `grep -n $'\u2014' <file>` on what you wrote and expect no hits. The dashes already in the tree stay: a prose change does not sweep them.

## Tickets

Tickets keep their own conventions: a ticket's description is written once, and a later pass appends to it rather than rewriting it. That is the convention: append by reading the ticket with `ticket_show` and passing the combined body in `ticket_set`'s `description`, after a blank line, because `description` **replaces** the stored body and a delta passed alone wipes everything else. The CLI's append flag does the same in one transaction at the scripts' door. An empty addition is a domain failure, and an empty `description` clears the body.

**Identity comes from the spawn site.** The actor a write records resolves from `TANDEM_AGENT` in the process that spawned the call (`rust/core/src/store/actor.rs`). The host harness's own subagent dispatch does not set it today, so agent writes there record `human`; that is a known gap, owned by TAN-336.

## Handoff

The actor that finishes a ticket's work sets the next status and, where there is a next, the next assignee instead of leaving the next move to a later pass. The names are the roster's: `examples/skills/tandem-team/SKILL.md`. The builder hands to `in-review`; the reviewer completes on merge or escalates to `human`.

**A pass narrates its handoff in one comment.** Before it moves a ticket's status, the pass appends one comment on every ticket it carried, through `ticket_comment_add`, signed with its own agent name. Three short fields: **what landed** (one line), **outcome and next status** (including the escalation, when there is one), and **issues** (anything blocking or discovered, or none). The comment narrates the status move; the `ticket_set` status write stays a separate act, and no status field, tool or parameter backs the contract. The comment log is the only work record; there is no separate usage or session face.

**Status and assignee move together, and `human` is an assignee only while a question is open.** The actor that closes a hold records the decision in a comment and moves the pair with `ticket_set`: the status the decision earns and the next owner move in the same call, and the ticket leaves the inbox with them. The inbox is the assignee alone, with no Answer-block check.

## Design

`DESIGN.md` at the repo root is the reference for the default, neutral style, taken from [awesome-design-md](https://github.com/VoltAgent/awesome-design-md) (MIT). Read it before drawing a new screen or component. The board ships two palettes, `neutral` (the default) and `tandem`, each in light and dark, and `rust/core/src/serve/static/tokens.css` stays the source of truth for every colour, space and radius: the style follows `DESIGN.md` for look and feel, and where the two disagree on a value the tokens win. Its marketing-site parts (hero bands, pricing tiers) do not apply to the board.

## Git

Prefer merges over rebases, and avoid force pushes and other destructive git actions unless absolutely necessary — when a force push is unavoidable, `--force-with-lease`, never bare `--force`. This applies to sub-agents too.

## Agents

The prompts for the passes live in `examples/agents/` (`builder`, `refiner`, `reviewer`, `security`, `architect`, `ux`, one markdown file each, named as nouns) and the procedures they run in `examples/skills/`. A prompt that names a contract states the current one; see [ADR 0045](docs/adr/0045-the-board-and-notes-cut.md). They are examples for this repository's own workflow: the cut retired the bundled agent face and `setup install`, and nothing installs them into a Tandem home.
