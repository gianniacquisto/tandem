# Members and personal tokens: identity comes from the credential

> **Status:** In force. See [the index](README.md).

A team workspace is people and their AI coding agents (tools such as Claude Code that connect to the board over MCP). Until now the board could not tell them apart: one shared `TANDEM_TOKEN` was one trust domain, and `X-Tandem-Agent` let anyone holding it write as anyone ([ADR 0047](0047-serve-is-the-single-door.md), cited by filename because two records share that number). The hosted team plan ([ADR 0048](0048-hosted-tandem-is-one-serve-per-workspace.md): EUR 4 per human seat, agents free) needs the board itself to know who each person is and which tools belong to whom, and it needs that in the open core so a self-hoster gets it too. This record is part 1 of CUT-36: members, personal tokens, sign-in keys and identity from the credential. What each member may do (roles checked in one place, the last-admin rule, the access record) is part 2 (CUT-39); the operator interface is CUT-40.

Decided 2026-11-16 (CUT-36). The human confirmed a personal token per person on 2026-11-19.

## Decision

- **Vocabulary.** A **member** is a person: a unique name and a role (`admin` or `member`). Members are the seats, and only members are seats: an AI coding agent is never a row of its own. A **personal token** is the credential a member gives an AI coding agent for its MCP settings; it belongs to one member and acts as that member. A **sign-in key** is the credential a member pastes into the login form of [ADR 0049](0049-a-browser-signs-in-to-a-token-guarded-serve.md) to get a browser session. The **owner** is whoever holds the server: loopback, `--allow-unauthenticated`, and the `TANDEM_TOKEN` holder. The **operator** is the hosting control plane's narrow key (CUT-40).
- **One principal, resolved once.** The guard resolves each request in one place into a closed kind, `owner`, `member-session`, `member-token` or `operator`, and leaves it in the request. `ResolveContext::resolve` only reads that principal and builds the actor: it never re-derives it and never reads the store, and no handler or tool looks at `Authorization` or the cookie. Fail closed: an unknown credential is refused.
- **Which credential reaches what.**
  - A personal token (`tdm_`) is admitted on `/mcp` and `/health` only; every other route answers it `403` with the reason in the log. So a token cannot reach the pages that export or manage people, by credential kind and not by header or role. `/mcp` still ignores the cookie.
  - A sign-in key (`tdk_`) is accepted only at `POST /login`: the form takes either the owner token or a sign-in key and mints a session that records the member (or no member for the owner). The key is never accepted as a bearer.
  - `TANDEM_TOKEN` stays the owner credential and keeps every route it reaches today. It is a second root credential beside the admins: the recovery and bootstrap path, not a member and not a seat.
- **Identity comes from the credential.** For a personal token or a member session the recorded actor is the member's name. `X-Tandem-Agent` absent: the member. Equal to the member's own name: accepted. Naming anyone else, including `human`: refused `403`, logged with a reason and no value. Roster names (`builder`, `refiner`) stay free text in `assignee` and `author` arguments, as today, and no longer name the actor of a member's writes. For the owner the header keeps ADR 0047's behaviour byte for byte. A member's name follows the actor rule (1 to 64 lowercase letters, digits and hyphens) and may never be `human`, `operator` or `owner`; a name is never reused, even after removal, so history never silently attaches to a stranger.
- **Team mode.** A home enters team mode when its first member is added, recorded as a persisted flag in the store (`store_meta`) that is never unset, so a deleted row cannot fall back to the loose header-is-attribution behaviour. A member credential that cannot be checked because the member tables are missing or unreadable is refused (fail closed), and the owner's credential still admits; an upgraded serve whose store has no member tables yet reads as no members. In `--allow-unauthenticated` mode a home that holds members refuses to start (exit 2): no credential is checked there, so it would silently ignore the members. Loopback with members: the local principal is the owner, as today. A home with no members behaves exactly as before.
- **Tokens and keys.** 32 random bytes from the operating system's generator (the generator ADR 0049 pins), base64url, behind a fixed visible prefix (`tdm_` for tokens, `tdk_` for sign-in keys) so a leaked one is greppable. A value of the wrong length or prefix is refused before any lookup. Stored as a SHA-256 digest only, looked up by that indexed digest. A plain hash is enough: the values are high-entropy random, so there is nothing for key-stretching to slow down, and the lookup leaks nothing usable. One `401` body and one code path for missing, malformed, unknown, revoked and removed, so a revoked token is indistinguishable from an unknown one (the log may differ). The throttle of ADR 0049 applies to a wrong member-shaped credential too, and a right one is never delayed. Debug on any type that holds a credential is redacted. `TANDEM_TOKEN` keeps its constant-time path and its 32-character floor and is never accepted in place of a member credential. A token's last-used time is rewritten at most once a day and, like every credential write, moves neither the board revision nor the rolling backup.
- **Storage.** Schema v14: `members` (with a tombstone, `removed_at`), `member_tokens`, `member_signin_keys` (one live key per member), and a nullable `member_id` on `browser_sessions` (an existing row has none and means the owner session, so a migrated serve signs nobody out). In an export `members` is carried and the two credential tables are emptied, as `browser_sessions` is. All SQL lives under `rust/core/src/store/` ([ADR 0009](0009-cli-agent-face.md)). Removing a member deletes their sessions, tokens and sign-in key in the same write, and the removal function is the one every door calls.
- **Revocation and removal take effect at once, open streams included.** Every request re-resolves its credential against the store: there is no in-memory credential cache. An `/events` stream authenticated by a member session closes after a removal or a revocation: an in-process signal closes it immediately, and a re-check on every revision and on a short timer closes it across processes (a local CLI removal), so a stream opened before the removal receives nothing after it. A removal during an in-flight MCP call lets that call finish and no later one.
- **The browser sign-in.** A session now also names its member, so a member's browser writes record that member; the owner session keeps the server's actor. The session digest key stays ADR 0049's, keyed by the owner token's digest, so rotating `TANDEM_TOKEN` signs everyone out: intended, because it is rare and safe. The instance function that opens a session for a named member is public inside the core so a later signed assertion (HOS-16) can call it; nothing here verifies an assertion.
- **The operator.** The operator's key (CUT-40) is narrow: it never acts as a member and never opens the board, and every use of it is recorded. This answers the question [ADR 0048](0048-hosted-tandem-is-one-serve-per-workspace.md) hands this ticket. The process owner can still read a volume, and this record does not hide that.
- **What is stated, not built here.** The human-only rule (no token ever reaches export or people management) holds by credential kind. The last-admin rule and the admin-by-operator decision are CUT-39 and CUT-40. The access record holds no secret and is not the work record of [ADR 0045](0045-the-board-and-notes-cut.md).
- **The rolling backup now holds credentials' digests.** [ADR 0018](0018-rolling-store-backup.md)'s `tandem.db.bak` is a whole-file copy, so it carries member rows and token and key digests. It is a copy of the home and is kept as private as the home.
- **The owner credential on a hosted fleet.** `TANDEM_TOKEN` is held by whoever runs the process; on the hosted fleet that is the operator's concern (HOS-17), and HOS-19 already states that the operator can technically read a volume.

## Supersedes

By quotation, in the same change, with the text checked against the files:

- ADR 0047, the decision bullet "The actor is per request": "the header is attribution, not authorization" is now true only for the owner.
- ADR 0047, the off-loopback bullet: "One token is one trust domain, so `X-Tandem-Agent` still names the actor on a token request" holds for the owner token only; a member credential names its own member.
- ADR 0049, Supersedes: "Per-actor tokens stay deferred to CUT-36": built here as personal tokens. (ADR 0049 already took the "browser login" half of ADR 0047's "Real authentication (per-actor tokens, a browser login) is a later ticket"; the two records do not quote the same words.)
- ADR 0049, the table bullet: "No member or owner column: CUT-36 adds one in its own migration" is done by schema v14.
- ADR 0047, Consequences: "TAN-336 narrows: an agent's session can name itself through the header; the harness's own subagent dispatch still sets no `TANDEM_AGENT` for the CLI." The gap named in the repository's `AGENTS.md` (subagent identity) has no live ticket: TAN-336 is archived and was about a retired executor. It stays open and unchanged by this work.

## Why

- **The board must know who is who before it can say who may do what.** Parts 2 to 5 build on a request that always comes from someone.
- **A personal token per person, not a shared team token or an agent account.** A shared token is today's problem again. An agent account would make agents seats, against HOS-1. A token that acts as its member and can never export or manage people keeps the rule that only humans administer.
- **Credential kind, not a role check, keeps a token off the dangerous pages.** A leaked agent credential cannot escape by being a different kind than it is.
- **Digest-only storage, with no cache.** A copy of the home (the backup, a volume) holds nothing that replays, and a revocation cannot be outlived by a remembered answer.

## Rejected

- Per-project or per-label access, roles beyond admin and member, agents as accounts an admin creates, a shared team token.
- A personal token that becomes a browser session, or a sign-in key that works as a bearer: it would let a leaked agent credential escape the human-only rule.
- A per-token label as the recorded actor (the label shows in lists and in the access record, never as the actor).
- `X-Tandem-Agent` silently ignored for a member: a refusal is louder and catches a mis-set agent.
- Letting the owner credential lapse once members exist: it is the recovery path and the bootstrap, and removing it would make a lost last admin a dead workspace.
- An in-memory credential cache, and per-member session keys that survive a `TANDEM_TOKEN` rotation.

## Consequences

- The owner credential is a standing all-powers credential held by whoever runs the process. That cost is accepted and stated here.
- A migrated store is refused by an older binary, as with every migration.
- Until CUT-39 binds export to the admin role, a member's browser session reaches the same pages the owner session does. A personal token never does.
- Part 2 to part 5 (CUT-39 to CUT-42) add the permission function, the operator interface, the personal tokens screen and the People screen.
