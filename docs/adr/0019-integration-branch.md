# Integration branch: `dev` takes merges, `main` tracks the latest tag

`main` was both the integration branch and the release line. A fix merged after a tag made `main` diverge from the shipped build, so "install what is on main" and "install the latest release" were two different acts, and the v0.13.0 → v0.14.0 sequence depended on the human remembering which was true.

We decided: **`dev` is the integration branch (merges land there) and `main` is the release line, its head always the latest release tag.** The first cut tagged `dev` and fast-forwarded `main` to that tag *(amended 2026-10-23: `main` takes its release moves by dev→main PR; amended 2026-10-29: the tag follows the merge, on `main`'s head, see the amendments below)*, and GitHub's default branch is `dev`, so a new pull request targets the integration branch without being told to.

## Why a line and a branch, not two working branches

- **One answer to "what is on main?"** — the latest shipped build. A post-tag merge can no longer put `main` ahead of the tag, because a post-tag merge cannot land on `main`.
- **External consumers read the release line** — clones and forks of the default branch see work in progress, where it belongs, while `main` stays a promise: every commit it names is shipped.
- **The pipeline needed no change**: the Release workflow fires on a `v*` tag push, branch-agnostic (ADR 0003), so a tag on either branch works as-is. The release-ordering rule from the done-mirror era is untouched: the release carrying the reader is installed before the destructive writer lands: release → install → land → release → install.

## Rejected

- **Keep `main` as the integration branch.** The status quo this ticket exists to end: `main` runs ahead of the installed build, and a merge after a tag is indistinguishable from shipped work.
- **A `release/*` branch per release, `main` staying integration.** A branch and a merge per release, for what a tag plus a fast-forward already answers — and `main` as integration is exactly what kept it drifting.

## Consequences

- The Flow (branch off the integration line → `swift build` + `swift test` → push → PR) is unchanged except that new pull requests target `dev`.
- `main` moves only by fast-forward to a tag *(amended 2026-10-23: by the release PR — see below)*. A `main` anywhere but a tag is a broken invariant, repaired by moving it to the latest tag.
- `docs/releasing.md` names the steps: tag on `dev`, push the tag, fast-forward `main` *(amended 2026-10-23: tag, publish, merge the release PR; amended 2026-10-29: merge the release PR, tag `main`'s head, the tag run publishes, see below)*.
- Executed 2026-10-20: `dev` pushed at `fbe6469` (equal to `main` at the time), and GitHub's default branch changed to `dev` the same day; this ADR is the record the ticket's done-when asked for.

## Amendment, 2026-10-23: `main` takes its release moves by dev→main PR

`main` is branch-protected as of 2026-10-22, so "fast-forwards `main`" no longer names the act: a raw `git push origin vX dev:main` fails with GH006, and the protection has not been relaxed since — v0.18.0 through v0.21.0 all ran the PR path. The release act is now: a lightweight tag on `dev`, the three-job pipeline publishes, then a dev→main release PR (title `Release vX.Y.Z`) merged once its `release` check is green — instantly, because the PR's head is the tagged commit, so the tag run's green check already attests it. `main`'s head is therefore no longer literally the latest tag: it sits one release-merge past it, its tree identical to the tag's (v0.21.0: merge `be94054`, parents `10bfae0` + `ee31858`, tree equal to the tag's).

The dev/`main` split itself is untouched: `dev` still takes merges, `main` is still the release line, and every commit it names is still shipped — the merge carries the tag's tree and nothing else. `docs/releasing.md` step 1 names the steps.

## Amendment, 2026-10-29: the tag follows the merge

The order above put the tag first and the merge second, which left a window: the release PR's head was "the tagged commit", and any PR merged into `dev` between the tag and the merge broke that head. v0.32.0 proved it in one day. It was tagged on `dev` at `52bab55`, #226 then landed on `dev`, and the release PR #227 no longer carried the tagged commit: it was closed and #228 opened from a branch pinned at the tag. Under the old order that race is structural, not bad luck.

The tag now follows the merge. The release act is: open the `Release vX.Y.Z` PR from `dev`, merge it once its `release` check is green, then tag `main`'s head and push the tag. The tag run publishes and re-attests the tree the PR run already built. `main`'s head is the release merge, so the tag names that merge and `main`'s head is literally the latest release tag again; a merge landing on `dev` after the PR cannot move anything the tag names, because `main` only moves by release PR.

Tags cut before this amendment stay on `dev` and are correct history: v0.32.0 is the last one, and nothing moves it. `Scripts/release-notes.sh` reads both orders: each tag resolves to the integration tip it names, the second parent when the tag sits on a release merge and the tag itself otherwise, so `<prevTip>..<newTip> --first-parent --merges` lists the same PRs a `dev`-tagged release listed. `docs/releasing.md` step 1 and `skills/tandem-sprint/SKILL.md` §7 state the new order.
