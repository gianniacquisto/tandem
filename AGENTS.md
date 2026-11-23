# AGENTS.md

## Work record

The ticket comment log is the work record: a pass narrates what landed, the outcome and next status, and any issues on every ticket it carried, and that comment is the only record kept. No pass record, usage report or capture layer sits beside it (ADR 0045).

**Agents call Tandem through the MCP tools**, never through the shell: `tandem-cli` is the human's and the scripts' door, documented in `docs/cli.md`; the tool conventions an agent needs are in `examples/AGENTS.md`.

## Prose

No em dash (U+2014) in anything an agent writes: use a comma, a colon, a full stop, or a hyphen instead. The ban covers PR bodies, review replies and issue comments; ticket titles, descriptions and comments; notes and captures; commit messages, code comments and docs. Before filing a comment or opening a PR, run `grep -n $'\u2014' <file>` on what you wrote and expect no hits; for a whole branch, `bash Scripts/check-prose-dash.sh` checks every added line. The dashes already in the tree stay: a prose change does not sweep them.

## Tickets

Tickets keep their own conventions: a ticket's description is written once, and a later pass appends to it rather than rewriting it. That is the convention: append with `ticket_set`'s `appendDescription` (the CLI's `--append-description`), which adds the text after a blank line and keeps the body. `description` **replaces** the stored body, and the two together are refused. An empty addition is a domain failure, and an empty `description` clears the body.

**Identity comes from the spawn site, or the request header.** The actor a write records resolves from `TANDEM_AGENT` in the process that spawned the call (`rust/core/src/store/actor.rs`), and for a call through `tandem serve`'s `/mcp` from the `X-Tandem-Agent` header, else the server's own actor (ADR 0047). The host harness's own subagent dispatch does not set it today, so agent writes there record `human`; that gap is outside this repo, the harness's to close, and no ticket here owns it.

## Handoff

The actor that finishes a ticket's work sets the next status and, where there is a next, the next assignee instead of leaving the next move to a later pass. The names are the roster's: `examples/skills/tandem-team/SKILL.md`. The builder hands to `in-review`; the reviewer completes on merge or escalates to `human`.

**A pass narrates its handoff in one comment.** Before it moves a ticket's status, the pass appends one comment on every ticket it carried, through `ticket_comment_add`, signed with its own agent name. Three short fields: **what landed** (one line), **outcome and next status** (including the escalation, when there is one), and **issues** (anything blocking or discovered, or none). The comment narrates the status move; the `ticket_set` status write stays a separate act, and no status field, tool or parameter backs the contract. The comment log is the only work record; there is no separate usage or session face.

**Status and assignee move together, and `human` is an assignee only while a question is open.** The actor that closes a hold records the decision in a comment and moves the pair with `ticket_set`: the status the decision earns and the next owner move in the same call, and the ticket leaves the inbox with them. The inbox is the assignee alone, with no Answer-block check.

## Design

`DESIGN.md` at the repo root is the reference for the default, neutral style, taken from [awesome-design-md](https://github.com/VoltAgent/awesome-design-md) (MIT). Read it before drawing a new screen or component. The board ships two palettes, `neutral` (the default) and `tandem`, each in light and dark, and `rust/core/src/serve/static/tokens.css` stays the source of truth for every colour, space and radius: the style follows `DESIGN.md` for look and feel, and where the two disagree on a value the tokens win. Its marketing-site parts (hero bands, pricing tiers) do not apply to the board.

- **The UI**: Askama templates under `rust/core/templates/`, CSS and JS under `rust/core/src/serve/static/`. A ticket that touches either is a UI ticket.
- **Tokens**: `--space-gutter`, `--space-row`, `--space-control`, `--space-label`, `--radius-card`, `--hairline`, and the colours. The four palettes are selected by `data-style` and `data-theme` on `<html>`, so a new token lands in all four sets with an AA check in each.
- **A refine mock** links `tokens.css` and `app.css`, and is checked in all four palettes.
- **Markup contract**: `tandem.js` holds the key handlers, and it and the htmx swaps name the template ids and classes.
- **Errors** reach the human through `rust/core/src/domain/error.rs` and the wire's `ErrorView`.
- **No undo**: completing a ticket cannot be undone (ADR 0045), so it counts as a destructive act for the `ux` lens.

## Build, verify and release in this repo

The prompts under `examples/` are generic: where one says the repo's AGENTS.md names a command, a path or a doc, the answer for Tandem is here.

- **Scoped tests**: in `rust/`, `cargo test -p <crate> <filter>`, or `cargo test -p tandem --test <suite>`. The full gates are in `CONTRIBUTING.md` (The gates), and CI runs them on every head sha.
- **Pre-push check**: `bash Scripts/check-prose-dash.sh`, because CI fails on an added em dash (see Prose).
- **Dev build**: `cargo build -p tandem` in your worktree. Serve that binary (`rust/target/debug/tandem`, never the installed `tandem`) in the background, on `TANDEM_HOME="$(mktemp -d)"` and a free `--port` that is never 7700, because the human's own serve holds it.
- **Token mode**: sign-in, people, token or operator work serves with `--host 0.0.0.0` and a generated `TANDEM_TOKEN` (`openssl rand -hex 32`). Open it at `http://localhost:<port>`, because the session cookie is `Secure`. Loopback is the default otherwise.
- **Doors**: a browser flow ends with a smoke pass: open the board and make one write. An MCP tool is called through `/mcp` on the dev serve, and a CLI verb runs against the dev home.
- **Public surface**: the MCP tools with their parameters and answer fields; CLI verbs, flags, output fields and exit codes; environment variables; and the HTTP routes `/health`, `/mcp`, `/events` and `/export`. A PR that renames, removes or changes the meaning of one gets the label `breaking` (`gh pr create --label breaking`), because the release notes list it first (`docs/versioning.md`).
- **CI artifact for a head**: `gh run list --branch <branch> --workflow release.yml --json databaseId,headSha,conclusion`, matched on `headSha`, then `gh run download <run-id> -n tandem-<os>`. Run it under a scratch `TANDEM_HOME`. The local build is `cargo build --release -p tandem` in `rust/`.
- **Release**: follow `docs/releasing.md`. Tag the landed `main` head, push the tag, watch the Release workflow publish, and generate the notes with `Scripts/release-notes.sh vX.Y.Z`. Once the tag is on origin, run `Scripts/pin-plugin.sh X.Y.Z` and land the one-line `.claude-plugin/marketplace.json` change on `main` through a PR, so the Claude Code plugin moves to the release.
- **Layers**: `rust/core` holds the domain and knows nothing of a UI; `rust/cli` is a face over it; `rust/core/src/serve` is the served HTML face (ADR 0009, ADR 0046).
- **Store**: `rust/core/src/store/` holds the SQL, and `rust/core/src/store/mod.rs` is its one writer. A change to `rust/core/src/store/schema.rs` lands with its numbered migration step in its own transaction (ADR 0031). The rolling backup `tandem.db.bak` is the only sanctioned copy of the database (ADR 0018).
- **Input guards**: shell commands are built in `rust/cli/src/main.rs`. A path built from input stays inside the guards in `rust/core/src/bikeshed/` and `rust/core/src/store/locked.rs`.
- **Docs**: `README.md`, `docs/*.md`, `GLOSSARY.md`, `DESIGN.md`, the `examples/` prompts and skills, and the ADRs in `docs/adr/`.

## Contributor agreement

The PR template's CLA checkbox ([CLA.md](CLA.md)) is required of every author except the copyright holder (`gianniacquisto`) and agents acting under that account. Any other co-maintainer ticks it.

## Git

Prefer merges over rebases, and avoid force pushes and other destructive git actions unless absolutely necessary — when a force push is unavoidable, `--force-with-lease`, never bare `--force`. This applies to sub-agents too.

## Agents

The prompts for the passes live in `examples/agents/` (`scout`, `refiner`, `builder`, `reviewer`, one markdown file each, named as nouns) and the procedures they run in `examples/skills/`. A prompt that names a contract states the current one; see [ADR 0045](docs/adr/0045-the-board-and-notes-cut.md). They are generic, so any repo can adopt them, and this repo's own commands and paths live under Design and under Build, verify and release in this repo. The cut retired the bundled agent face and `setup install`, and nothing installs them into a Tandem home.
