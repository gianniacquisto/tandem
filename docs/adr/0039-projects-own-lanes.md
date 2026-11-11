# Projects own lanes; a lane has exactly one parent

> **Amended 2026-11-11 by [ADR 0043](0043-the-board-draws-one-axis.md):** the board no longer groups lanes under project headers. It draws one column per workflow stage, a lane is a chip on the card and a filter, and the project is the focus the columns are narrowed by. This record's model (a project holds lanes, a ticket derives its project via its lane) stands unchanged. The amendment's text is ADR 0043's.

> **Amended 2026-11-08 by [ADR 0041](0041-workflows-are-data.md):** `projects.workflow` is the later hook for a per-project workflow. Projects still own lanes unchanged; this record's model gains the nullable column and no reader yet. The amendment's text is ADR 0041's.

Project and lane are coupled but not the same entity. A project holds several
lanes, and every lane has exactly one parent project. Refs stay lane-keyed
(`TAN-42`), sprint identity stays `(lane, number)`, and the default area stays
project-less capture.

Decided 2026-11-03 (TAN-295).

## Decision

- **A `projects` table, and a parent on every lane.** `projects` holds `name`
  (primary key), `position` (render order, add appends, no move verb) and
  `author`; `lanes` gains `project`. The v6 migration backfills one project
  per existing lane (project name equals lane name, order follows lane order)
  and sets each lane's parent. Default-area tickets are untouched; lane names,
  keys, numbers, refs and positions are unchanged.
- **The domain owns the parent rule; the ticket carries no project.** A
  ticket's project is derived via its lane, never stored and never a second
  column. The default area has no project: cross-project capture, first on the
  board.
- **Sprints, sessions and note captures are untouched.** Sprint identity stays
  `(lane, number)` with no project column. `--project` on session and note
  verbs stays free text and unvalidated.
- **The board snapshot is unchanged.** The renderer joins `project list` for
  the mapping instead of growing the snapshot. The board groups lanes under
  project headers in project order, default area first and ungrouped, empty
  projects not drawn.

## Why

- A second truth on tickets would need lane moves to keep it in step;
  derivation via the lane follows the ref precedent.
- Validating session and note `--project` against the table would couple
  worktree directories and bikeshed folders to the store; filesystem truth
  stays separate.
- Requiring `--project` on `lane add` would break every existing call; the
  default (project named for the lane) keeps them green and still satisfies
  one-parent-each.
- Deleting a project that still holds lanes is refused, naming those lanes, so
  a delete can never strand or silently re-home numbered lanes.
