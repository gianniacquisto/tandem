# Origin: who filed a ticket is stamped at add, reads as human when absent, and never changes

[ADR 0017](0017-the-ticket-model.md) recorded that *origin* (who added a todo) is not *assignee* (who is on it) and deferred it. It is now in: a ticket's **origin** is who filed it — `human`, or the name of the agent that filed it.

## Why

The list mixes tickets the human typed with tickets agents file, and nothing distinguishes the two: an inbox of agent proposals looks identical to the human's own intentions, so triage cannot start with "what did I ask for?". The "an agent filed this for you" queue the human wants has no data to queue over until the filer is recorded on the ticket.

## Decision

- **One additive field, `origin`**: the string `human` or an agent's name. Written once, at `add`, and **immutable** — `Todo` declares it `let`, so the type itself enforces the rule, and `set` has no origin to change. **Absent reads as `human`**: every pre-existing ticket predates the field, and the human's own list was filed by the human.
- **The spelling rules are `assignee`'s**: trimmed; empty means `human`; the reserved word is stored lowercase so it is one name; an agent's name is stored as given. Nothing validates the name — the persona registry that would is out of scope, as with `assignee` (ADR 0017).
- **The store writes it always, never `null`**: a ticket's filer is always known, so the first write after the field exists stamps it on every ticket, and the file shows the whole shape rather than a shape that varies by ticket.
- **`tandem-cli todo add` gains `--origin <name|self>`**, with `self` resolving to `$TANDEM_AGENT` exactly like `--assignee self`. The flag is **optional; an absent flag means `human`** — the same neutral default `--assignee`'s absent state gets. The skill tells an agent that files a ticket to pass `self`: an absent flag is the app's truth (the app is the human's face and files as human), and an agent that omits it files its own proposal under the human's name.
- **The ticket sheet shows the filer read-only**, beside the assignee: "you filed", or the agent's name in colour — the agent's filing is the signal triage starts from. No list or board chrome in this change; the "filed for you" queue is a future view over the field.

## Compatibility — why the reader ships first

`origin` is additive: an older reader ignores the unknown key, and the envelope stays `version: 1` ([ADR 0006](0006-stored-order-and-sections.md)). But an **older writer drops the key on every write**, because its encoder writes only the fields it knows. A ticket an agent filed would read back as human — silently, and with no way to tell it lost anything — undoing the field's whole purpose if an upgrade lands out of order. The gate is the same reachability question as the `done` retirement (ADR 0017): **install the build that reads the field before any writer starts storing it** — release, install, then the writers follow. No migration is needed: absent reads as human, and the first write stamps the value, which is a reconciliation, not a recovery (no file is copied aside).

## Decided against

- **Reusing `assignee` as origin** — who is on a ticket and who filed it are different questions: a ticket an agent files *for* the human carries both, and the conflation loses the filer.
- **A convention in the description** (a "Filed by: blue" line) — prose is not data: no filter can key off it, the sheet has no field to show, and every writer has to remember the format.

## Consequences

- Every `add` — app and CLI — stamps `origin`; the app files as `human`, the CLI as its `--origin` says.
- The store file and the CLI's wire carry the field identically (unlike the derived `done`): there is no second shape to keep honest.
- The `agentNameUnset` error is now flag-neutral — it can come from `--assignee self` or `--origin self`.
- A stale writer that drops the key is the recorded risk the install order prevents; the loss is silent, so the order is the only protection.
