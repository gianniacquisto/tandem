# One entity, the ticket — the vocabulary rename

The store, the app, the CLI and the docs described one object in two words: the code and the GUI had moved to **ticket**, while the CLI's verb, the store's on-disk keys and the prose still said **todo**. A session had to learn which word was real, and the word a developer typed was the older one. This ADR records the two decisions that ended the split — the verb (D1) and the on-disk names (D2) — and the reader-first order that shipped them without breaking a reader.

## Why

`GLOSSARY.md` carried the split as definitions: **Todo** was "the word the store and the CLI use", **Ticket** was "a Todo carrying the full shape" — two entries for one entity. Every other surface said ticket: the board skill, ADR 0017, the ticket sheet, the statuses, the lanes.

The verb could not simply be renamed. `todo` is the published agent surface — `skills/tandem/SKILL.md` documents it, the agent prompts run it, and every skill on this Mac is a symlink into this repo, so a doc edit is live before any build is installed. Renaming the verb without an alias breaks agents mid-flight, or leaves them typing a word the installed binary rejects.

The on-disk names could not simply be renamed either. The file is read by the installed app, the bundled CLI, and human hands, and the failure mode is not a graceful degrade: a reader that does not find `todos` calls the file **corrupt**, moves it to `todos.json.corrupt-<ts>` and starts empty (the 2026-10-17 incident, ADR 0006). A reader that does not find `section` loses every ticket's lane silently, and the next write makes the loss permanent.

## Decision

- **One entity, called Ticket** — in the type, the app, the CLI verb, the wire keys, the store keys and the docs. **D1's answer is A**: `ticket` is the verb and `todo` is a deprecated alias for one release — same parse, same output, one line on stderr saying so; the `list` reply carries `tickets` and `todos` with the same array. The release after the alias's release drops both.
- **The store's on-disk names change — keys and file.** **D2's answer is B**: `todos` → `tickets`, `sections` → `lanes`, per-ticket `section` → `lane`, and the file becomes `tickets.json`. The envelope stays `version: 1`.
- **The order is reader-first**, so no installed reader ever meets a word it cannot read:
  1. **C4** — the decoder accepts both spellings; the writer still emits v1. Ships in the reader build.
  2. **R1 installed** (v0.16.0) — the CLI answers `ticket`; the app copy says Tickets. Only now may the agent-facing prose say the new word: a doc that says `ticket` against a binary that rejects it is the failure the gate exists to prevent.
  3. **C5** — the writer emits `tickets`/`lanes`/`lane`, after the reader build is installed. Shipped in v0.17.0 (R2).
  4. **The file rename** — read `todos.json`, write `tickets.json`, remove the old file only after the new one is durable. Its own ticket (`FA494ACA`) with its own verification; **pending**.
- **The docs collapse with the code** — `GLOSSARY.md` defines **Ticket** alone; the README's store section shows the keys the writer actually emits; the tracker doc and the agent-facing skills say `ticket`. Each gates on the install that makes its words true.

## Decided against

- **D1-B, a hard cut** — `ticket` only, `todo` a usage error. Every prompt and script that says `todo` fails on the install, with no hint; the doc pass would have to land in the same release and be right the first time.
- **D1-C, keep `todo` as the only verb** — the type, the app and the docs say ticket while the word a developer types stays todo; the split survives at the one surface agents touch daily.
- **D2-A, keys rename and the file name stays** — workable, but it leaves the hand-editable file named for a word nothing else uses; the human chose the full rename (B).
- **D2-C, no on-disk change** — zero migration, but every `jq`, hand-edit and pasted snippet still says todo; the split moves one layer down instead of closing.
- **Rewriting the older ADRs** — 0001, 0006, 0010, 0017 and 0021 say `todo`/`section` because that is what was decided then; they are history. This ADR states that "Todo" in an earlier record means Ticket; the glossary is the current authority. ADR file names are not renamed either — the links that cite them are load-bearing.
- **Renaming the skills' paths** (`skills/tandem` stays) — the skill's name is the product's, not the entity's.

## Consequences

- **The decoder keeps both spellings forever**, like `done`: a file from any point in the migration reads, and a reader is never stranded by a writer half a step ahead.
- **The alias's end is C8** (`CB772112`): retire the `todo` verb and the `todos`/`section` wire keys once the doc passes (C6, C7) have merged and the release carrying them is installed here. Until then the verb prints its deprecation line and the `list` reply carries both keys; after it, one word, one key.
- **The decoder's permanence and C8 are the same fact read twice**: the decoder's tolerance outlives the alias, because files written before the rename keep existing on disk — a hand-restored backup, a stale checkout — and must still read. What retires is the CLI's acceptance of the old *verb*, not the store's tolerance of the old *keys*.
- **The file-name rename keeps its own failure mode in sight**: the store file is the one file the human cannot afford to lose, so the move is read-old, write-new, remove-old-only-after-durable — never an in-place rename.
- **The number.** 0030 was the first free number: 0027 is claimed by the session-records ADR, 0028 by the store-rename ADR, 0029 is the ceremony set. The sprints ADR (`02CA3EC2`) also claimed 0030 — the repo's rule settles it: the older ticket keeps the number, and the newer branch renumbers on its own branch.
