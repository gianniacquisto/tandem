# Throwaway Branch Workflow

How wayfinder work gets captured on this Mac: the branch is an asset, not a candidate for merge. Used by every map (`tandem`, `bikeshed`, `tandem-ui`, `distribution`, `tandem-memory`).

## The shape

- **Research** findings land on `research/<name>`; **prototype** artifacts on `prototype/<name>`. Both cut from `main`, both pushed, neither merged, no PR opened.
- The **verdict** — what the work settled and where its assets live — is recorded in the ticket (the map's tracker), and a context pointer to the branch rides on the same ticket.
- `main` keeps only the validated decision: the one-line gist in the map's *Decisions so far* plus the ticket's verdict section.

## Why unmerged

The decision is the deliverable, not the code or the notes. Merging would imply the assets are production and would pull throwaway state into `main`'s history; leaving the branch unmerged keeps the asset reachable (it is pushed) while the tree stays clean.

## Gotchas

- A placeholder `#<n>` cross-reference auto-links to an *unrelated* issue — wire cross-references in a second pass, after the issues exist and have real ids.
- An unpushed throwaway branch is a lost asset: push it as soon as the first asset commits, even if more will follow.
