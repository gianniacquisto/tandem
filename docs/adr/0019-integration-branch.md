# Integration branch: `dev` takes merges, `main` tracks the latest tag

`main` was both the integration branch and the release line. A fix merged after a tag made `main` diverge from the shipped build, so "install what is on main" and "install the latest release" were two different acts, and the v0.13.0 → v0.14.0 sequence depended on the human remembering which was true.

We decided: **`dev` is the integration branch — merges land there — and `main` is the release line, its head always the latest release tag.** A release tags `dev` and fast-forwards `main` to that tag, and GitHub's default branch is `dev`, so a new pull request targets the integration branch without being told to.

## Why a line and a branch, not two working branches

- **One answer to "what is on main?"** — the latest shipped build. A post-tag merge can no longer put `main` ahead of the tag, because a post-tag merge cannot land on `main`.
- **External consumers read the release line** — clones and forks of the default branch see work in progress, where it belongs, while `main` stays a promise: every commit it names is shipped.
- **The pipeline needed no change** — the Release workflow fires on a `v*` tag push, branch-agnostic (ADR 0003), so tagging `dev` works as-is. The release-ordering rule from the done-mirror era is untouched: the release carrying the reader is installed before the destructive writer lands — release → install → land → release → install.

## Rejected

- **Keep `main` as the integration branch.** The status quo this ticket exists to end: `main` runs ahead of the installed build, and a merge after a tag is indistinguishable from shipped work.
- **A `release/*` branch per release, `main` staying integration.** A branch and a merge per release, for what a tag plus a fast-forward already answers — and `main` as integration is exactly what kept it drifting.

## Consequences

- The Flow (branch off the integration line → `swift build` + `swift test` → push → PR) is unchanged except that new pull requests target `dev`.
- `main` moves only by fast-forward to a tag. A `main` anywhere but a tag is a broken invariant, repaired by moving it to the latest tag.
- `docs/releasing.md` names the steps: tag on `dev`, push the tag, fast-forward `main`.
- Executed 2026-10-20: `dev` pushed at `fbe6469` (equal to `main` at the time), and GitHub's default branch changed to `dev` the same day; this ADR is the record the ticket's done-when asked for.
