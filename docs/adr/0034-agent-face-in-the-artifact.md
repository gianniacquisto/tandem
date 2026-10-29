# The artifact ships the agent face, and `tandem-cli setup` is its only writer

A skill and the CLI it drives have to be the same release, and until now they were not. A release shipped `Tandem.app` with `tandem-cli` inside it, and the agent face (the fourteen skills under `skills/`, the four prompts under `agents/`, and `AGENTS.md`) existed only in a git checkout on somebody's disk. The setup was a manual act on that checkout: `skills/tandem-setup/` walks an agent through linking `~/.config/opencode/skills/<name>`, `~/.config/opencode/agents/<name>.md` and `~/.config/opencode/AGENTS.md` into it. The app's setup wizard told a human to run `ln -s "$PWD/skills/tandem" <host>/tandem`, a path only a checkout has, and `doctor`'s `skills` row looked for that route or an `opencode.json` `skills` array.

On a second Mac, or on this one after the checkout moved, that setup is the workflow, not the setup: the CLI on `PATH` comes from the app, and the skills it is documented by come from wherever the clone happens to sit. A release could change the CLI's grammar and the skill would still describe the old one. A deleted or repointed checkout left links that resolved to nothing, and nothing reported them.

The fix is the one the artifact already implies: the release carries the agent face, so a skill and the CLI it drives are always the same version, and one writer installs it.

## Decision

- **`Scripts/make-app-bundle.sh` copies `skills/`, `agents/` and `AGENTS.md` into `Tandem.app/Contents/Resources/agent/`, before it signs the app.** The copy lands before `codesign --force --sign -`, so the ad-hoc seal covers the face and the script's own `codesign --verify --strict` still passes. The script writes `Contents/Resources/agent/VERSION` from its `<version>` argument (`printf '%s'`, no trailing newline): that marker is the one version truth, and nothing reads `Info.plist` for it.
- **The release workflow asserts the face is intact in the artifact.** The `Verify: bundle structure` step reads the checkout's own `skills/*/SKILL.md` and `agents/*.md` and asserts each is present under `Contents/Resources/agent/`, plus `AGENTS.md` and a `VERSION` equal to the version the run resolved. The loops read the disk, so a skill added tomorrow is asserted without a number in YAML going stale. The workflow runs on `pull_request`, so a PR dry run exercises the step.
- **`<TandemHome>/agent/` is the installed copy.** A real directory, materialised beside the old one as `agent.new-<uuid>` and renamed into place, with `VERSION` inside it. The same version means the copy is left alone; a different version, a missing marker, or a file set that does not match the bundle means a wholesale replace. The replace is atomic: on any failure the temporary tree is removed and the old copy and every link stay as they were.
- **The host paths are symlinks into it.** Per host, exactly these and no more:
  - `opencode`: `$XDG_CONFIG_HOME/opencode/skills/<name>` for every skill, `$XDG_CONFIG_HOME/opencode/agents/<name>.md` for every prompt, and one `instructions` entry in `$XDG_CONFIG_HOME/opencode/opencode.json` naming `<TandemHome>/agent/AGENTS.md`. `XDG_CONFIG_HOME` defaults to `~/.config`.
  - `claude`: `~/.claude/skills/<name>` for every skill, and a marked block in `~/.claude/CLAUDE.md` holding `@<TandemHome>/agent/AGENTS.md`.
  - `agents`: `~/.agents/skills/<name>` for every skill, and no instruction file.

  `opencode` is the default. `claude` and `agents` are written only when `--host` names them, because each is a directory the human may not use, and `skills/tandem-setup/` already says another host is the human's call.
- **`~/.config/opencode/AGENTS.md` is never written.** The `instructions` entry replaces it as the installed route. On a machine where that path already links a checkout, install leaves it alone and `setup status` warns `double-load`: removing the old link is the human's call.
- **The link rule, in one table.** Absent: created. A symlink resolving into `<TandemHome>/agent/`: `ok`. A symlink whose target is gone: replaced (a dead link is not a decision). A symlink resolving anywhere else, a checkout included: skipped with `points elsewhere: <target>`, replaced only with `--force`. A real file or directory: skipped with `a real file is there`, never replaced, `--force` included, because the flag is about the checkout route and a real file is the human's work.
- **`<TandemHome>/agent-install.json` is the manifest.** It sits beside `agent/`, never inside it, because `agent/` is replaced wholesale and the record of what was installed must outlive the replace. It holds `version`, `hosts` (the hosts a run targeted, independent of what it managed to write), `links` (`path`, `kind`, `target`, and the host it belongs to), and `edits` (`file`, `kind`, `text`, `host`), each `text` the exact bytes written. Install merges into it, keyed by `path` and by `file` plus `kind`, so `--host opencode` then `--host claude` keeps both records; it is rewritten atomically, temp file plus `rename(2)`.
- **`tandem-cli setup` is the face's only writer and uninstaller.** Three verbs, one JSON report shape:
  - `tandem-cli setup install [--host <opencode|claude|agents>] [--force]`
  - `tandem-cli setup uninstall [--host <opencode|claude|agents>]`
  - `tandem-cli setup status`

  Uninstall removes what the manifest records, in order: each recorded link that still resolves into the face, its own `instructions` entry, its own `CLAUDE.md` block, then the face and the manifest. It never deletes a real file or directory and never removes a path it did not record. A narrowed run (`--host`) takes only that host's records and rewrites the manifest with the rest. With no manifest it removes the face and reports the host links unattributed rather than guessing.
- **The app reaches the same writer, not a copy.** The setup wizard's agent step and Settings' Agent section call `SetupInstall.status` and `SetupInstall.install` in-process, the functions behind `tandem-cli setup status` and `tandem-cli setup install`: the app adds no path of its own, draws the report's own state, and writes nothing to a host without a press. The wizard's two checkout snippets go with it.
- **The bundle resolves in one place.** `SetupInstall.bundle(environment:executablePath:isDebugBuild:)`: `TANDEM_AGENT_BUNDLE` when set and non-empty (the test seam, the shape `TANDEM_HOME` already has), else `<that .app>/Contents/Resources/agent` when the executable runs inside a release `.app`, else none. A dev build cannot invent a bundle.
- **`setup status` and `doctor` read one state.** `SetupInstall.state(home:environment:bundle:)` returns the bundled version, the installed version, each host's state (`installed`, `checkout`, `real-file`, `absent`) and the warnings. `setup status`, `setup install` (after it writes) and `SetupDiagnostics.observe()` all call it, so the three cannot disagree.
- **The OpenCode config is read tolerantly and written textually.** The live config is JSONC (comments and trailing commas), and the host parses it, so the reader strips comments and trailing commas from a copy to answer "is the entry there". The write is a textual insert: `"instructions": ["<entry>"],\n` after the top-level `{` when the key is absent, `, "<entry>"` before the array's `]` when it is present. The manifest records the exact inserted text, so uninstall deletes exactly that string and the file returns byte-identical. A file neither JSON nor JSONC is skipped with its reason, its skills still linked.
- **A checkout still wins on a development machine.** ADR 0026's "the working tree is the deployment" bullet holds, now with the checkout on `main`: on the machine working on Tandem, the host links resolve into the checkout rather than the installed face, install reports them as skips (`foreign-link`, `info`), and `doctor` stays green. The installed copy is for the machine that has the app and not the repo.

## The warning model

One record shape, `{code, severity, path, detail, hint}`, produced by `SetupInstall.state` alone. `path` is `nil` for a face-level code; it is what makes "one per path" and "severity, then code, then path" expressible. `Code` is an enum with a `String` raw value, `CaseIterable` and `Codable`; the raw strings are the contract, pinned by one test that lists all fourteen. Severity is a field of the record, not of the code, because two codes are context dependent.

| code | severity | when |
| --- | --- | --- |
| `no-bundle` | `warn` inside a `.app` whose `Contents/Resources/agent` is absent, `info` otherwise | no bundle resolved |
| `no-manifest` | `warn` | `<home>/agent/` exists with no `agent-install.json` |
| `stale-face` | `warn` | the installed `VERSION` differs from the bundle's |
| `missing-version` | `warn` | `<home>/agent/` has no `VERSION`; suppresses `stale-face` and `partial-face` |
| `partial-face` | `warn` | the installed skill directories or prompt files differ from the bundle's set |
| `dead-link` | `warn` | a scanned host path is a symlink whose target is gone |
| `orphan-link` | `warn` | a link into the face that the manifest does not record |
| `foreign-link` | `info` | a link resolving outside the face: the checkout route |
| `checkout-shadows-install` | `warn` | the same, while a current install exists and is shadowed |
| `blocked-path` | `warn` | an expected host path holds a real file or directory, on a host an install targeted |
| `double-load` | `warn` | `~/.config/opencode/AGENTS.md` exists beside the `instructions` entry |
| `host-disabled` | `warn` | a host is installed and an OpenCode variable disables it (`OPENCODE_DISABLE_CLAUDE_CODE`, `OPENCODE_DISABLE_CLAUDE_CODE_SKILLS`, `OPENCODE_DISABLE_EXTERNAL_SKILLS`; truthy only for `true` or `1`, case-insensitive) |
| `half-installed` | `warn` | a targeted host has its links but no instruction file, or its instruction file but no links |
| `unwritable` | `error` | the home or a host directory is not writable, probed on the nearest existing ancestor |

The scan covers the four host directories, the two instruction files and the OpenCode config. A path is Tandem's, and therefore reported, only when its name is one the bundle ships, the manifest records it, or it resolves into the face; anything else in those directories belongs to another tool. Per path the first match wins, one record for the path: a real file or directory (`blocked-path`), a symlink whose target is gone (`dead-link`), a symlink resolving outside the face (`foreign-link` or `checkout-shadows-install`), a symlink into the face the manifest does not record (`orphan-link`). `dead-link` beats `orphan-link` on purpose: a link to a retired skill inside a replaced face is both, and the repair is the same.

`half-installed` applies to the two hosts that have an instruction file; `agents` has none, so its links are its whole state.

Exit codes: `setup status` exits `0` always, warnings in the report. `setup install` prints the same list after a run and exits `0` when the warnings are `warn` or `info`, so a skip is visible at the moment it happens; it exits `1` when a warning is an `error` (`unwritable`), with the report still on stdout, because the report is the diagnosis. A thrown write failure keeps the older contract: the message on stderr, no JSON on stdout, exit `1`.

## `doctor`'s eighth row

`agent-face` is appended last in `SetupDiagnostics.checkNames`, so every existing index and the documented prefix stay put. It is red while any `warn` or `error` stands, and its detail is the count, the first code and that code's hint (`1 warning: dead-link, tandem-cli setup install`), or `no warnings`. The `skills` row keeps its `ok: true` and its `opencode configured` substring, gains the route (`(installed)` or `(checkout)`) and the hint `install: tandem-cli setup install`, and reads the same state, so the two rows cannot disagree.

The consequence, stated: `tandem-cli doctor` now exits `1` while a `warn` stands, which is what the Health panel needs. On a machine with a dead host link, the `## Verify` block in `skills/tandem-setup/` still prints its line and the command's exit code moves.

## Why

- **A skill and the CLI it drives must be one release.** The skill states the CLI's grammar; shipped apart they drift, and the drift is invisible because nothing checks the installed pair. Shipping them from one tag makes a mismatch impossible rather than unlikely.
- **One installed copy, not one per host.** An upgrade is one directory replace and no re-linking, and every rule reads "resolves into `<TandemHome>/agent/`", so turning that directory into a `current` symlink later keeps every rule true and changes only the resolver.
- **`/Applications` is not state.** Linking host directories straight into `Tandem.app/Contents/Resources/agent` would dangle during an upgrade that replaces the bundle. `<TandemHome>/agent/` survives the replace, and the links point at it.
- **A real file is the human's.** The one rule that needs no detection is "outside `<TandemHome>/agent/` is somebody else's link". That covers a checkout, another tool's link, and a hand-made file, and it lets `--force` mean exactly one thing.
- **A warning nobody can act on is noise.** `no-bundle` is `info` on a dev build, because the checkout route is the normal state on this repo's own machine and a `warn` would turn `doctor` red every day for a state nobody can fix there. Inside a `.app` without the face it is `warn`: that is the artifact bug this ADR exists to prevent.
- **Textual config edits preserve the human's file.** Re-serialising `opencode.json` destroys its formatting and comments and cannot be undone byte for byte; an exact insert with the exact text recorded can.

## Consequences

- **The bundle grows by about 208 KB** (`skills/` 180K, `agents/` 20K, `AGENTS.md` 8K), and `NOTICE.txt` already travels with it, covering the vendored MIT skills.
- **`TANDEM_AGENT_BUNDLE` is a new environment variable**, honoured only by `setup` and `doctor`: a test fixture and a deliberate override, in the shape `TANDEM_HOME` set.
- **Uninstall leaves the directories install created when they are empty.** A directory is not state, and pruning one risks removing a directory the human made; every file, link and edit returns to its pre-install bytes, and the manifest and the face are gone.
- **The manifest is a new file in Tandem's home**, beside `tandem.db`, the bikeshed and the lockfiles. Nothing else reads it.
- **The shipped `AGENTS.md` is written for the Tandem repo.** A global install applies its Tandem-specific lines (the `## Git` section, the repo layout) to every project on the machine. One file is kept because two variants drift; the human's own instruction files coexist because OpenCode combines the `instructions` files with whatever `AGENTS.md` it finds, which is why this writes neither over the other.
- **`opencode.jsonc` is out of scope.** The route is `opencode.json` only; a machine that keeps its config in `opencode.jsonc` is reported, not edited.
- **A dev build installs into the dev home.** `<TandemHome>/agent/` follows `TandemHome` like everything else, so a `.build/debug` CLI writes `~/.tandem-dev/agent/`. That is the rule, not a bug: the same binary already reads `~/.tandem-dev/tandem.db`, and on a machine working on Tandem the host links resolve into the checkout and are skipped anyway.

## Deferred

- **Versioned install directories plus a `current` pointer.** One `<TandemHome>/agent/` for now: the face is version-locked to the binary, both shipping from one tag, so a mixed state cannot arise, and a rollback is the app's rollback, which replaces both together. The upgrade path is recorded in the Consequences above.
- **A marketplace or plugin wrapper, so `claude plugin update` can refresh the face.** It needs the `claude` CLI present and its own versioning; not built here.
- **Notarization.** Accepted as recorded in ADR 0003: none for now, ad-hoc signature, trust granted at download.

## Decided against

- **A second formula for an agent-only install.** ADR 0003 deferred it, and the cask already links the in-bundle CLI.
- **Linking the host directories straight into the app bundle.** An upgrade replaces the bundle, so those links dangle during the replace.
- **A copy per host.** One installed copy means an upgrade is one directory replace.
- **Copying only `tandem` and `tandem-memory`.** The whole `skills/` and `agents/` ship; a host that finds a skill can run it.
- **Hardcoding the skill count in the workflow.** A number in YAML is a build failure the day a skill is added; the loop reads the disk.
- **A `--bundle` flag for the fixture.** A test seam does not belong in the product's grammar; `TANDEM_AGENT_BUNDLE` is the shape `TANDEM_HOME` already set.
- **Wiring every host the machine happens to have.** Writing `~/.claude` for a human who never uses Claude Code is a side effect nobody asked for.
- **`--force` overriding a real file.** The flag is about the checkout route; a real file is the human's own work.
- **Detecting "a checkout" by path pattern or a `.git` probe.** "Outside the face is somebody else's link" needs no detection.
- **Changing `doctor`'s `opencode configured` string.** It is quoted in `skills/tandem-setup/` and pinned by a test; the route and the new hint go after it.
- **A glossary entry per host.** Hosts are a list that will grow; the word that needed defining is **Host**.
- **A JSONC parser dependency.** Stripping comments and trailing commas on a copy answers the only parse question asked, and the write is textual.
- **Re-serialising `opencode.json`.** It destroys the human's formatting and comments and cannot be undone byte for byte.
- **Reading `Info.plist` for the version at runtime.** The bundle script already has `$version` on its command line, so the marker it writes is the one truth.
- **A second `AGENTS.md` for a host.** Two variants drift; one file, and the ADR says what it costs.

## Amendments

- **ADR 0026's setup bullet.** That bullet says the setup wires `~/.config/opencode/AGENTS.md` by convention and that "a host config edit is needed" is not. It now reads: the setup is `tandem-cli setup install`, the face travels in the artifact, OpenCode takes an `instructions` entry in its config (the former global `AGENTS.md` route is the double-load warning), and a checkout still wins where one exists. The rest of ADR 0026 stands: the prompts live in `agents/`, and nothing in this repo names a path on one machine.
- **ADR 0003's bundle contents.** The prebuilt artifact now carries `Contents/Resources/agent/` beside the two binaries, `Info.plist`, the icon, `NOTICE.txt` and the editor bundle. The distribution decision (one cask, in-bundle CLI) is unchanged.
- **ADR 0014's check list.** The list is eight rows, not seven, and the new row can fail: `agent-face`. ADR 0018's rejection of a seventh row is not contradicted, because that row could never fail; this one can. The report's shape and the pass/fail rule are unchanged, and the `skills` row stays informational.
- **The trunk is `main`** (ADR 0019, amended 2026-10-29). The release is a tag on `main`'s head; no branch named `dev` exists, and nothing here names one.
