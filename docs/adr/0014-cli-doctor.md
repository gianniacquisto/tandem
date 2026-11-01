# A setup verb outside the domains: `tandem-cli doctor`

> **Amended 2026-11-01 by TAN-243:** the check list is eleven. Three informational rows join directly after `comments` and name the JSON-era leftovers an imported store retired: `sessions` (`sessions.json` and its `.pre-sqlite-*` copy), `settings` (its own), and `todos-json` (the `todos.json.*` cohort, one row for the whole set). Each prints the files it found and one `rm -f` with every path quoted by the same helper the comments and store rows use, or a line saying the home has none; none can fail, so the report's `ok`, the exit codes and the pass/fail rule are unchanged. The rows appear only beside a database, because ADR 0031's rule leaves the port no import marker: the store's schema is the marker.

> **Amended 2026-10-29 by [ADR 0034](0034-agent-face-in-the-artifact.md):** the check list is eight: `agent-face` joins as the last row, and it can fail, unlike the informational rows ADR 0018 kept out. The report shape, the names, the order and the exit codes are unchanged; the row reads `SetupInstall.state`, the same producer as `setup status`. The amendment's text is ADR 0034's.

> **Amended 2026-10-26 by [ADR 0031](0031-one-database-the-store.md):** the check list is seven — `comments` joined — and `lock` keeps its name and its non-destructive rule but probes a write transaction instead of a sidecar `flock`; the `store` row names `tandem.db` and its schema version. The names, the order, the report shape and the exit codes are unchanged. The amendment's text is ADR 0031's.

[ADR 0009](0009-cli-agent-face.md) fixed the agent surface to `todo list|add|complete` plus `note rename`, under the rule "verbs are cheap to add and expensive to withdraw". Adding one therefore needs an argument. `doctor` is not a domain verb: it reads no todo and writes nothing. It reports *the machine's setup* — where the state lives, and whether the store, the bikeshed, the lock, the installed app, and the agent skill are sound — so it earns a place **outside** the `todo` / `note` namespaces, as a bare `tandem-cli doctor`.

## Why

- **First run is where Tandem is most likely to be misconfigured, and it is the least observable.** Which home applies (shipped `~/.tandem`, a dev build's `~/.tandem-dev`, or a `TANDEM_HOME` override) is decided by how the binary was built and where it sits; the quarantine flag silently SIGKILLs the CLI until it is stripped; the `tandem` skill may not be installed for the agent host; a corrupt store, or a bikeshed path that is a file, breaks the next write. Today the only way to learn any of this is to read four documents and try a `todo list`. One command that names all of it is the first-run surface the CLI was missing.
- **It is read-only, so it does not widen the agent's write surface.** It takes no lock and writes no file; the lock check is a non-destructive `flock` probe that releases immediately. The domain verbs stay the only things that mutate state.
- **Outside the domains is the honest place for it.** Under `todo` or `note` it would read as acting on todos or notes; it acts on neither. A bare verb says what it is.
- **The logic lives in the tested core.** Gathering is impure (it stats the filesystem and probes the lock), but it produces a value and `SetupDiagnostics.report(_:)` is a pure function of it, so the report's shape, wording, and pass/fail rule are covered by `TandemCoreTests` with injected facts. That keeps the executable a thin adapter — the same rule that made the architecture review decline moving the store's wire-parsing into the CLI.

## Rejected

- **Fold it into `todo` or `note`.** It belongs to neither domain; the names describe what the verb acts on, and this one acts on the installation.
- **Human-readable text only, no JSON.** The contract is one JSON object on stdout, always; a setup script or an agent should be able to branch on the result without parsing prose.
- **Exit `0` unconditionally (report everything as information).** The value is that a broken store, a held lock, or a bikeshed path that is a file makes the command *fail* — that is what a first-run check is for.
- **Put the gathering and the pass/fail decision in the executable.** That would be untested boundary logic, exactly what ADR 0009 and the review keep out of the CLI.

> **Amended 2026-10-21:** the ground of this rejection moved — the CLI's own path is now reachable by the test suite, so logic in the executable would be tested, not untested. The decision is unchanged: the core keeps the gathering and the pass/fail rule, and the CLI stays a thin adapter. See the amendment below.

## Consequences

- The output shape `{"ok": bool, "checks": [{"name", "ok", "detail"}]}` and the **check order** (`home`, `store`, `bikeshed`, `lock`, `app`, `skills`) are a contract, documented verbatim in the `tandem` skill. Renaming a check or reordering is a breaking change to it.
- Three checks can fail — `store` (present but unreadable/corrupt), `bikeshed` (the path is a file), `lock` (held) — and `ok` is the AND of those. `home`, `app`, and `skills` are informational: a missing app or an uninstalled host is context, not a failure.
- New read-only core surface: `Store.health()` (counts, or `absent` / `unreadable`, with **no** recovery — a diagnostic never backs up or rewrites), `Store.lockIsHeld()` / `LockedDirectory.isLocked()`, and `TandemHome.Source`, which names which branch of the home rule a process took (the rule itself now derives from it, so the two cannot drift).
- The README and the skill name the new verb; `docs/releasing.md` is unaffected.

## Amendment, 2026-10-21: the rejection's ground moved, the decision did not

The rejected option above — putting the gathering and the pass/fail decision in the executable — rested on "untested boundary logic". **That ground is now false.** [ADR 0009](0009-cli-agent-face.md)'s amendment of the same date records why: the single test seam now depends on the `tandem-cli` product (`Package.swift`), so the CLI's own path is reachable by the suite — the adapter's functions in process, the built binary end to end. Logic in the executable would now be tested, not untested. The "Why" bullet above is untouched by this: the logic does live in the tested core, and the core is still where it lives.

**What is unchanged.** The decision stands: the core keeps the gathering and the pass/fail rule, and the CLI stays a thin adapter that calls them unchanged. What it now rests on is ADR 0009's boundary rather than the tests' reach. `SetupDiagnostics.gather()` observes the machine and `SetupDiagnostics.report(_:)` is the pure function that turns those facts into the report — the pass/fail rule's one home, covered by `TandemCoreTests` with injected facts. A second home for that rule in the executable would not be an untested one; it would be a second copy of a rule that has one home. And the adapter's own header is explicit about what belongs to the executable — *"the two things that belong to a process rather than to the domain live here: stdin and the environment"* — while the doctor's gathering is impure but is the machine's setup rather than a process's stdin or environment; the facts it observes come from the core (`Store.health()`, `Store.lockIsHeld()`, `TandemHome.Source`).

Nothing about `doctor` changes: the verb, the report shape, the check order and the exit codes are as they were, and this amendment changes no behaviour anywhere.
