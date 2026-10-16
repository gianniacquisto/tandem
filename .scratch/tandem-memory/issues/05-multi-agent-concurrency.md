# 05 - Multi-agent concurrency: two harnesses, one vault

Type: grilling
Status: open
Blocked by: 01

## Question

OpenCode and Claude Code can work the same project — or the same shared topic notes — at the same time, and agent writes take no lock (ADR 0004: the sidecar lock is one-sided, for the app; agents write plain files, last write wins; the app's mine/theirs banner covers app buffers only, not agent-vs-agent). Settle the conventions the skill encodes so concurrent sessions do not lose each other's highlights:

1. **Ownership**: which notes are shared (project state, topic knowledge) vs per-session scratch; whether per-agent or per-session notes exist at all, and how their content merges back into shared notes.
2. **Write discipline**: re-read immediately before write; minimal-diff amendments; append-only sections. What a session does when it detects another session's changes landing mid-amendment (agents have no UI to resolve in — the convention must be a rule).
3. **Collision inventory**: the realistic collision scenarios (two sessions on one repo; two sessions on different repos touching one shared topic note; a session amending a note the other is about to rewrite) and the convention each needs.
