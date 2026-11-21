# Architecture decision records

One file per decision, numbered in the order they were taken. A record is never rewritten: a later decision amends or supersedes it, and says so at the top of the older file.

Tandem has changed shape twice since the early records: the Swift app became a Rust core, and [ADR 0045](0045-the-board-and-notes-cut.md) and [ADR 0046](0046-tandem-serve-and-single-binary-distribution.md) then cut the product to a board and a notes store served by one binary. Many records describe surfaces that no longer exist. This index says which ones still bind.

## Start here

The records that explain the product as it is today:

- [0031](0031-one-database-the-store.md): the store is one SQLite database.
- [0018](0018-rolling-store-backup.md): the rolling backup beside it.
- [0017](0017-the-ticket-model.md) and [0030](0030-one-entity-the-ticket.md): the ticket and its vocabulary.
- [0035](0035-ticket-dependencies.md): the blocking edge.
- [0039](0039-projects-own-lanes.md), [0047, lanes are labels](0047-lanes-are-labels.md) and [0053](0053-one-prefix-per-project.md): projects and labels, and who owns the ref prefix.
- [0004](0004-vault-protocol.md), [0005](0005-wikilink-rename-rewriting.md), [0008](0008-vault-path-rule.md) and [0022](0022-bikeshed-move.md): how notes are stored, named, renamed and moved.
- [0033](0033-wiki-is-documentation.md): what the notes are for.
- [0045](0045-the-board-and-notes-cut.md): what was cut, and what stayed.
- [0046](0046-tandem-serve-and-single-binary-distribution.md): `tandem serve`, the single binary and how it is distributed.
- [0047, serve is the single door](0047-serve-is-the-single-door.md): agents reach Tandem over MCP on `tandem serve`; how requests are guarded and named.
- [0049](0049-a-browser-signs-in-to-a-token-guarded-serve.md): how a browser signs in to a token-guarded serve.
- [0052](0052-members-and-personal-tokens.md): members, personal tokens and sign-in keys; identity comes from the credential.

## How to read the status column

The status comes from each record's own header and from the "Prior records" list in ADR 0045. Nothing here was decided by this index.

- **In force**: no later record supersedes it. Where a later record amends it, the amendment is named. This is the record trail, not a fresh check of the code.
- **Superseded**: a later record replaces the decision, named in the row.
- **Surface retired** or **Overtaken**: no record supersedes it by name, but ADR 0045 retired the surface it describes, or current practice has moved on. Read it as history.

## Index

| # | Record | Status | Notes |
| --- | --- | --- | --- |
| 0001 | [Store protocol](0001-store-protocol.md) | In force, amended by 0031 | The JSON file, sidecar lock and envelope are retired for the store. What 0031 left standing is in its header. |
| 0002 | [App distribution](0002-app-distribution.md) | Superseded by 0003 | Homebrew formula and cask link. |
| 0003 | [Distribution](0003-distribution.md) | Superseded by 0046 | Prebuilt cask. |
| 0004 | [Vault protocol](0004-vault-protocol.md) | In force | Per-note files, the sidecar lock, atomic writes. |
| 0005 | [Wikilink rename rewriting](0005-wikilink-rename-rewriting.md) | In force | A rename made through Tandem rewrites the links that pointed at the note. |
| 0006 | [Stored order and sections](0006-stored-order-and-sections.md) | In force, amended by 0031 | The order now lives in `position` columns. |
| 0007 | [Locked directory](0007-locked-directory.md) | In force | One primitive owns the lock and atomic-write mechanics. |
| 0008 | [Vault path rule](0008-vault-path-rule.md) | In force | One validation rule for note paths. |
| 0009 | [The agent face is a CLI](0009-cli-agent-face.md) | In force as amended by 0047 (single door) | Agents now reach Tandem over MCP on `tandem serve`; the CLI is the human's and the scripts' door. The "one dispatch" rule stands. |
| 0010 | [A todo may link to a note](0010-todo-note-links.md) | Surface retired by 0045 | `tickets.link` and ticket-to-note promotion were cut. |
| 0011 | [Chosen vault](0011-chosen-vault.md) | Surface retired by 0045 | The settings that stored the choice were cut; the notes live at `<home>/bikeshed`. |
| 0012 | [Editor formatting](0012-editor-formatting.md) | Superseded by 0045 | Amended earlier by 0042. |
| 0013 | [Bikeshed folders](0013-bikeshed-folders.md) | In force | Folders are plain directories. |
| 0014 | [`doctor`](0014-cli-doctor.md) | In force as amended | Seven checks: home, store, integrity, references, space, versions, bikeshed. Read-only. |
| 0015 | [Bikeshed, not vault](0015-bikeshed-not-vault.md) | In force | The name of the notes folder. |
| 0016 | [Note capture](0016-note-capture.md) | Superseded by 0045 | The capture verb and the hidden `.raw/` layer. |
| 0017 | [The ticket model](0017-the-ticket-model.md) | In force as amended | Amended by 0031, 0041 and 0043. Status is six fixed stages again since 0045. |
| 0018 | [Rolling store backup](0018-rolling-store-backup.md) | In force, amended by 0031 | The backup is `tandem.db.bak`. |
| 0019 | [Integration branch](0019-integration-branch.md) | Overtaken | Its `dev` integration branch is gone: [CONTRIBUTING](../../CONTRIBUTING.md) makes `main` the trunk that pull requests target. No record supersedes this one by name. |
| 0020 | [Origin field](0020-origin-field.md) | In force | Who filed a ticket is stamped at add. |
| 0021 | [A note can become a ticket](0021-note-to-todo.md) | Surface retired by 0045 | Promotion between notes and tickets was cut. |
| 0022 | [Bikeshed move](0022-bikeshed-move.md) | In force | A move is the same transaction as a rename. |
| 0023 | [Reveal the hidden layer](0023-reveal-the-hidden-layer.md) | Surface retired by 0045 | The capture layer it reveals was cut. |
| 0024 | [Session ticket workflow](0024-session-ticket-workflow.md) | Superseded by 0027 | |
| 0025 | [A ticket can become a note](0025-todo-to-note.md) | Surface retired by 0045 | Promotion between notes and tickets was cut. |
| 0026 | [Agent prompts and portable setup](0026-agent-prompts-and-portable-setup.md) | Superseded by 0045 | The prompts live on as examples under `examples/`. |
| 0027 | [Session records](0027-session-records.md) | Superseded by 0045 | A pass is recorded in the ticket comment log. |
| 0028 | (none) | Never landed | The number was claimed by a store-rename record on a branch that did not merge (see 0029 and 0030, "The number"). |
| 0029 | [The ceremony set](0029-the-ceremony-set.md) | Superseded by 0045 | The comment log survives as the work record. |
| 0030 | [One entity, the ticket](0030-one-entity-the-ticket.md) | In force | The vocabulary. |
| 0031 | [One database, the store](0031-one-database-the-store.md) | In force as amended | No `store_imports` table ships; the store gained `store_meta` and its revision. |
| 0032 | [Sprint records](0032-sprint-records.md) | Superseded by 0045 | |
| 0033 | [The wiki is documentation](0033-wiki-is-documentation.md) | In force | What the notes hold and how a page stays true. |
| 0034 | [Agent face in the artifact](0034-agent-face-in-the-artifact.md) | Superseded by 0045 | The binary ships no agent face and no installer for one. |
| 0035 | [Ticket dependencies](0035-ticket-dependencies.md) | In force, amended by 0045 | The edge and the derived block stand; the `tickets.link` and sprint-draw clauses retired. |
| 0036 | [The tag bumps the tap](0036-the-tag-bumps-the-tap.md) | Superseded by 0046 | |
| 0037 | [One stack: Rust core, Electron UI](0037-one-stack-rust-core-electron-ui.md) | Superseded by 0046 | The Rust core stands; the Electron shell retired. |
| 0038 | [The harness home](0038-the-harness-home.md) | Superseded by 0040 | |
| 0039 | [Projects own lanes](0039-projects-own-lanes.md) | In force as amended | Its "lane" is today's label (0047). The board no longer groups by project (0043). Refs are project-keyed and there is no default area (0053). |
| 0040 | [No embedded agent loop](0040-no-embedded-agent-loop.md) | In force | Tandem runs no agent; orchestration stays outside it. |
| 0041 | [Workflows are data](0041-workflows-are-data.md) | Superseded by 0045 in its data half | The workflow tables and custom stages retired. |
| 0042 | [The bikeshed editor is a block editor](0042-the-bikeshed-editor-is-a-block-editor.md) | Superseded by 0045 | The served notes page has its own editor in `rust/core/src/serve/static/tandem.js`; no record describes it yet. |
| 0043 | [The board draws one axis](0043-the-board-draws-one-axis.md) | In force, amended by 0045 | One column per stage, with the six fixed stages. Its "lane" is today's label. |
| 0044 | [Agent, skill and lens](0044-agent-skill-lens.md) | In force, amended by 0045 | The lenses stand as examples under `examples/`. |
| 0045 | [The board and notes cut](0045-the-board-and-notes-cut.md) | In force | |
| 0046 | [Tandem serve and single-binary distribution](0046-tandem-serve-and-single-binary-distribution.md) | In force, amended by 0047 (single door) | Its "two server doors", "no HTTP JSON-RPC" and "loopback is the authentication" clauses are superseded. |
| 0047 | [Lanes are labels](0047-lanes-are-labels.md) | In force as amended | Schema v12. The key and counter moved from the label to the project (0053). |
| 0047 | [Tandem serve is the single door](0047-serve-is-the-single-door.md) | In force, amended (CUT-37, CUT-36) | MCP over HTTP on `tandem serve`; `tandem mcp` is retired. Shares its number with the record above: two branches took 0047 and both merged. Both stay as numbered, and the index check accepts the pair; a renumbering is a human call. |
| 0048 | [Hosted Tandem is one `tandem serve` per workspace](0048-hosted-tandem-is-one-serve-per-workspace.md) | In force | The hosted product is the published binary, one serve and one home per workspace, behind a closed control plane that holds no content. Records the boundary; the siblings own their parts. |
| 0049 | [A browser signs in to a token-guarded serve](0049-a-browser-signs-in-to-a-token-guarded-serve.md) | In force | A session cookie held by the instance, a keyed digest in `browser_sessions` (schema v13). Supersedes the browser-login half of 0047. |
| 0050 | [The home leaves and arrives as one archive](0050-the-home-leaves-and-arrives-as-one-archive.md) | In force, amended (CUT-39) | `tandem export` and `tandem import`: one streamed `.tar.gz` of the store and the notes, no secrets, restored only into an empty home; one export permission check and a record line. Amends 0047's list of verbs with no remote form; 0018's "no restore verb" stands for the rolling `.bak`. |
| 0051 | [A Claude Code plugin is the connector](0051-a-claude-code-plugin-is-the-connector.md) | In force | The repository is a Claude Code marketplace; the `tandem` plugin is `examples/` pinned to the latest release tag: it registers the MCP endpoint, ships the example skills and agents, and checks at session start that serve is up and current. No Tandem-owned installer (0045), serve still started by hand (0047). |
| 0052 | [Members and personal tokens](0052-members-and-personal-tokens.md) | In force | Members (admin or member), personal tokens (`tdm_`, `/mcp` only) and sign-in keys (`tdk_`, `/login` only), stored as digests in schema v14; the actor comes from the credential, and team mode fails closed. Part 2 (CUT-39): one permission function, the last-admin rule, and the append-only access record (schema v15). Supersedes by quotation parts of 0047, 0049 and 0050. |
| 0053 | [One prefix per project](0053-one-prefix-per-project.md) | In force | The project owns the ref key and the counter (schema v16); a ticket stores its project and never changes it; there is no default area; a label is optional inside one project; retired refs resolve through `ticket_aliases`. Amends 0039 and 0047. |

## Adding a record

Take the next free number, add the file with a `> **Status:** ...` line under its title, and add its row here in the same change. `rust/core/tests/adr_index.rs` fails when a record has no row or no status line. When a record supersedes or amends an older one, say so at the top of the older file and update that row too.
