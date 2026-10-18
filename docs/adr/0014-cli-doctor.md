# A setup verb outside the domains: `tandem-cli doctor`

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

## Consequences

- The output shape `{"ok": bool, "checks": [{"name", "ok", "detail"}]}` and the **check order** (`home`, `store`, `bikeshed`, `lock`, `app`, `skills`) are a contract, documented verbatim in the `tandem` skill. Renaming a check or reordering is a breaking change to it.
- Three checks can fail — `store` (present but unreadable/corrupt), `bikeshed` (the path is a file), `lock` (held) — and `ok` is the AND of those. `home`, `app`, and `skills` are informational: a missing app or an uninstalled host is context, not a failure.
- New read-only core surface: `Store.health()` (counts, or `absent` / `unreadable`, with **no** recovery — a diagnostic never backs up or rewrites), `Store.lockIsHeld()` / `LockedDirectory.isLocked()`, and `TandemHome.Source`, which names which branch of the home rule a process took (the rule itself now derives from it, so the two cannot drift).
- The README and the skill name the new verb; `docs/releasing.md` is unaffected.
