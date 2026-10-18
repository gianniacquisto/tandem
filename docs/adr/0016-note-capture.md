# A second note verb: `tandem-cli note capture`

[ADR 0009](0009-cli-agent-face.md) fixed the agent surface to `todo list|add|complete` plus **exactly one** notes verb, `rename`, under the rule "verbs are cheap to add and expensive to withdraw". This adds a second: `tandem-cli note capture --kind <decision|gotcha|fact|work|thread> [--project <name>] [--session <id>]`, whose body is read from stdin and which prints `{"path": "<bikeshed-root-relative path>"}`. It writes one **raw memory note** into a hidden, per-project capture layer at `projects/<project>/.raw/<session>/<utc>-<kind>-<slug>.md`.

The verb exists to serve the raw-capture protocol (`.scratch/tandem-memory/raw-layer-design.md`, decisions signed off 2026-10-18). Measured, the memory protocol records into only three curated tiers, so every agent amends the same few anchors and sub-agents — which see no protocol and no private, conflict-free place to write — record **nothing** (0 of 70 sub-agent sessions). The fix is a hidden layer of many small, create-only notes written by every agent as work happens, with a consolidation pass as the sole writer of the curated notes. `capture` is that layer's write path.

## Why `capture` earns a verb where `create` did not

- **It passes ADR 0009's own test — semantics the filesystem cannot own.** ADR 0009 kept `create` out because its invariant is a single filename a careful agent can reproduce. `capture`'s filename encodes a naming and queue invariant that no file tool can hold: a UTC stamp (`yyyyMMdd'T'HHmmss'Z'`) that orders the queue so `existence = undigested`, a canonical slug derived from the body's first line, and **collision-freedom** — a same-second, same-slug note gets a `-2`, `-3`, … suffix, where a shell `>` would silently clobber it. The queue is the point of the layer; losing a note breaks it.
- **One tested home for the invariant.** The alternative for each agent host is a shell snippet that re-derives the stamp, slug, and collision rule — untested, and duplicated per host. The design rejected that (decision 9) for the same reason ADR 0009 keeps the store's wire-parsing in the core.
- **It is a naming/queue rule, not a note-content interface.** `create`, `write`, and `delete` stay rules and non-verbs exactly as ADR 0009 left them; `capture` writes only into its own hidden layer, so it never becomes a second way to author a curated note.

## Why the layer is hidden and the write lives in the core

- **Out of the graph by construction, not by rule.** The `.raw/` component is hidden, and `Bikeshed.isHidden` prunes the whole subtree from the tree, `notePaths`, `search`, backlinks, and wikilink resolution. A captured note therefore cannot pollute the curated graph even if an agent forgets — the property is code-enforced.
- **Hidden space needed a new core write path.** `BikeshedPath.url` already tolerates hidden components; only `validateNewName` rejects them, and it is the stricter *create/rename* rule. So `capture` does its own single-component check on `project` and `session` (empty, `.`, `..`, a leading dot, `/`, `#`, `|` are rejected) and adds the `.raw` component itself. The new `Bikeshed.capture` operation owns the path, slug, UTC stamp, and uniqueness, under the existing bikeshed lock. The CLI stays a thin adapter (ADR 0009): it reads stdin, applies the `--project`/`--session` defaults, and serializes the returned path.

## Rejected

- **A skill shell script instead of a verb.** Every agent host would re-implement the stamp/slug/uniqueness rule, and none of it would sit behind the single test seam. Rejected by the design (decision 9).
- **Reuse `create` with a hidden path.** `validateNewName` rejects hidden names on purpose, and `create` treats a taken target as an error rather than suffixing — the opposite of capture's queue rule.
- **Fail on a same-second collision rather than suffix.** A collision here is two genuine moments in one second, not a mistake; failing would lose the second note and push the agent back onto the filesystem.
- **A `note append` to one note per session.** The design chose one file per moment so the consolidation queue is `existence = undigested`; appending would reopen a write boundary and a lock the create-only rule avoids (decision 3).
- **Take the body as an argument instead of stdin.** Bodies are multi-line prose; stdin keeps them off the command line and out of shell history.

## Consequences

- **New public core surface**: `CaptureKind` and `Bikeshed.capture(project:kind:body:session:at:)`, plus two `BikeshedError` cases (`invalidCaptureName` for a bad project/session, `invalidCaptureKind` for an unknown `--kind`). An unknown `--kind` is a **domain** failure (exit 1), not a usage one, matching the existing "a missing flag value is left to the domain" rule.
- **The CLI gains one verb**: `note capture`, with the body read from stdin and the result printed as `{"path": "<path>"}` exactly like `rename`. `--project` defaults to the working directory's last path component; `--session` defaults to `$OPENCODE_SESSION_ID`, else a short generated token.
- **ADR 0009's "notes get exactly one verb" is amended, not erased**: the note verb set is `rename` **and** `capture`, while every argument for keeping `create` / `write` / `delete` out still stands.
- **No store shape, version, or GUI change** — so this rides any release with no version-bump hazard and no rebuild-then-install ordering, and `version` stays `1`.
- The `tandem` skill documents the verb; the raw-capture protocol, the `AGENTS.md` orientation rule, and the consolidation pass are separate text work.

Amends [ADR 0009](0009-cli-agent-face.md) — the `note` namespace is `rename` + `capture`; the rejection of a full notes CLI (`create`/`write`/`delete`) is unchanged.
