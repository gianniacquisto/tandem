# The wiki is documentation: one audience contract for humans and agents, and the page roles that serve both

The value review (`4147A3ED`, 2026-10-26) put the question to the human and got one answer: the bikeshed is **for both readers** — a real project wiki a human reads and acts on, and the orientation surface an agent opens before it touches code. A cold read of the vault made the same point from the other side: the Overview's `Where left off` and the Risks, Gotchas and Decisions pages carried every fact the work needed, while the Index's forty-five `Next:` lines carried almost none, and several were stale. A stale `Next:` is worse than no line, because a human treats it as status and an agent treats it as truth. [ADR 0004](0004-vault-protocol.md) settled the vault's protocol and [ADR 0015](0015-bikeshed-not-vault.md) settled that the bikeshed is not a second vault; neither says who each page is *for*. This record does.

## Decision

- **One wiki, two readers.** The curated notes under `projects/**` and `topics/` are documentation, written in prose a human can read start to finish, and they are the orientation surface a session reads first ([`AGENTS.md`](../../AGENTS.md)). The two jobs are not served by writing twice: a page that states its role plainly is readable by both.
- **The wiki holds current truth and decisions.** Current truth is what the project is, how to run it, where things live, and what is open now. Decisions are the ADRs' — referenced with a short rationale, never copied; the ADR stays in the repo, and [ADR 0004](0004-vault-protocol.md)'s index-not-copy rule already says so.
- **The journal is out.** The narrative of what was tried, in what order, and with what result is not a wiki page. Git history is that record, and the ended session records ([ADR 0027](0027-session-records.md)) are the prose one. A page holds no changelog, no dated diary and no `Next:` line.
- **Each page has one job.**

  | Page | Its job |
  | --- | --- |
  | **Index** | pointers and one-line status — the way in, never the detail |
  | **Overview** | what this is, how to run it, where things live, and what is open (`## Threads`, plus `Where left off`); human-readable prose, and the agent's entry point |
  | **Decisions** | one dated line per decision, linked to its ADR or its ticket |
  | **Risks** | one dated line per open risk; a retired risk leaves the page |
  | **Processes** | one dated line per process, linked to the note or ticket that documents it |
  | **Sub-pages and topics** | project documentation proper — install, first run, the CLI's groups, the app's tabs, the recovery paths — one coherent topic per note |

- **The README is the front door.** It installs the app and the CLI, and it points at `~/.tandem/bikeshed/projects/<project>/<project>.md` for the rest, so a human reaches a project's Overview — install, first run, where things live, what is open — without reading an ADR.
- **The freshness rule.** A page names the date it was last true (`Reviewed <date>`) or carries none, and the Notes tab's mtime is the reader's signal. A pass that changes a fact amends the page in the same write, as `AGENTS.md` already requires for the session's capture: the page is corrected where it was read, not left for a later pass and never contradicted by a second appended line.
- **The page-role list lives in the conventions file.** `skills/tandem-memory/CONVENTIONS.md` is where the pages are described and where the freshness rule is stated; `AGENTS.md` points at it rather than restating it. Nothing new is stored — the Notes tab already draws the scaffold, and no field, verb or table changes.

## Why

- **The vault already served both, by accident.** The pages that answered a human's question were the pages that oriented an agent; the page that served neither was the Index's per-project `Next:` line, which was a journal kept one line at a time. Naming the audience makes the accident the contract.
- **A stated role is what keeps a page small.** "One job" is a budget as much as a description: the Overview links out instead of deepening, the Decisions page indexes instead of arguing, and a retired risk is deleted rather than dated in place.
- **The README cannot carry a project's current state.** It is the front door for the product, not for a project's live threads; the Overview is where the reader arrives and the state is true as of its last review. One line in the README is the whole cost of the bridge.
- **A dated page is a page a reader can trust.** Without a date, a stale page reads exactly like a fresh one. `Reviewed <date>` and the mtime give the reader the one thing that decides whether to act on what they read.
- **The record of work already exists twice.** Git history holds what changed; the session records hold how the work went ([ADR 0027](0027-session-records.md)). A journal page would be the third copy, and the one nobody updates.

## Decided against

- **Agents-only.** The human reads this vault too, and a page written only for an agent — terse, all `Next:` lines, no install path — is not documentation.
- **A separate human wiki.** Two vaults diverge; one file with two readers is the cheaper contract, and the second vault is the one that rots unwatched.
- **Freezing the pages.** A wiki nobody updates is worse than a board; the freshness rule, not a freeze, is what keeps trust.
- **Keeping a journal page, or the Index's `Next:` lines, and dating it.** The narrative is already in git and in the session records; a dated journal is still a page every pass must rewrite and every reader must discount.
- **Moving the ADRs into the wiki.** The repo's decisions stay in `docs/adr/`, reviewed by pull request; the wiki indexes them ([ADR 0004](0004-vault-protocol.md)).
- **A new skill for the contract.** The pages are already described in `CONVENTIONS.md`, which `SKILL.md`, `CONSOLIDATION.md` and `AGENTS.md` all reach; a skill would be a fourth door to the same rules.

## Consequences

- `CONVENTIONS.md` gains the audience contract, the page-role list and the freshness rule; `AGENTS.md`'s Memory section names it in the protocol line instead of restating the roles.
- The README gains the wiki's entry point, so the human's path is README → Overview.
- The consolidation pass writes to the roles: `Where left off` and `Threads` refresh, a retired risk leaves its page, and a page whose facts changed gets its `Reviewed` date in the same write.
- The Index stays a pointer page: it shrinks as its `Next:` lines go, which is `573178F9`'s build, and its one-line statuses stay derived from the pages it points at.
- The scaffold test pins the load-bearing phrases — the page roles, the journal's exclusion and the freshness stamp — so a rewrite cannot drop the contract silently, in `Tests/TandemCoreTests/ScaffoldConventionTests.swift`.
