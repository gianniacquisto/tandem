# 03 - Read and orientation protocol: picking up, and finding related information

Type: grilling
Status: open
Blocked by: 01

## Question

Given the layout and entry point settled in 01, settle the read protocol the skill encodes:

1. **Orientation**: what a new session reads, in what order, before doing work — the entry point from 01, then which notes — and how much to load: enough to pick up where the work left off, without burning the context window.
2. **Retrieval**: how a session finds information related to the topic it is working on — wikilinks, backlinks, plain search of the vault (the agent can grep `~/.tandem/bikeshed/`; the app's full-text search is the same operation with a UI) — and the order of preference among them.
3. **Staleness**: what a session does when what it reads contradicts the current state of the repo — re-verify, correct the note, or work around it; and what it does when a wikilink is broken (the note was renamed or deleted).
4. **Freshness during a session**: how often a long session re-reads its notes, or when it may trust its earlier reads.
