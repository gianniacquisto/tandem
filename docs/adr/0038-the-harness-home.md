# The harness keeps a home of Tandem's, and the environment is the seam

The app runs its own agent loop by spawning a bundled `opencode` binary, so the store the harness writes must sit in Tandem's home and never in the one the human's own editor uses. The binary makes that harder than `app.name` suggests: every global path is `join(root, "opencode")` with `root` fixed at build time (`const app = "opencode"` in upstream `packages/util/src/global.ts`), so `app.name` is cosmetic for paths, and a naive spawn writes `~/.local/share/opencode/{auth.json,snapshot,tool-output,worktree}`, `~/.config/opencode` and `~/.cache/opencode`.

The seam is the environment. Upstream's `global-roots.ts` resolves `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME` and `XDG_STATE_HOME` once, at module load, and `os.tmpdir()` for the one root it does not take from XDG. A child spawned with those set inherits them, and the binary itself carries `OPENCODE_DB` and `OPENCODE_CONFIG_DIR` for the two paths the SDK calls `database.path` and `config.directory`. That was the claim; this record is what the spike measured.

Decided 2026-10-31, against `opencode` v2.2.5, arm64 macOS, `sha256 abe4d88594bffb12b364375b8a6732d2d1f7a977195ed1a35c5c76f8e89c9e0e`. The check is `Scripts/check-harness-home.py` and it is the proof.

## Decision

- **The bundled binary is pinned by exact version and digest.** `2.2.5` and the digest above live in `Scripts/check-harness-home.py`; CI verifies the release asset against it before the binary is packaged (its own ticket), and nothing upgrades at runtime: the binary carries an `upgrade` subcommand and no path from the app reaches it. The artifact is one 226,703,008-byte Mach-O linking only `libicucore`, `libresolv`, `libc++` and `libSystem`, signed Developer ID `5NZ4Q7NXJ4` with the hardened runtime and a timestamp, so it ships verbatim: no re-sign, no `codesign --deep` over the bundle.
- **The harness is spawned as `opencode serve --stdio --port 0`.** It is its own process, its lifetime bound to its stdin: closing the pipe ends it with exit 0, where a plain `serve` outlives its stdin. The app reads the ephemeral url from the first stdout line (`{"url":"http://127.0.0.1:<port>"}`), and holds the credential: `OPENCODE_SERVER_PASSWORD` in the child, `Basic base64("opencode:<password>")` on each request.
- **The whole environment is fixed at spawn.** One table, copied into the app's launcher and asserted by the check:

| variable | value | what it roots |
| --- | --- | --- |
| `XDG_DATA_HOME` | `<harness>/data` | `auth.json`, `snapshot/`, `tool-output/`, `worktree/`, `shell/`, `log/`, `repos/` |
| `XDG_CONFIG_HOME` | `<harness>/config` | the config root when `OPENCODE_CONFIG_DIR` is absent |
| `XDG_STATE_HOME` | `<harness>/state` | service and session state |
| `XDG_CACHE_HOME` | `<harness>/cache` | `bin/`, the extracted tooling |
| `TMPDIR` | `<harness>/tmp` | `opencode/` and Bun's `.bun-*.dylib` |
| `OPENCODE_DB` | `<harness>/opencode.db` | the harness database, exactly this file |
| `OPENCODE_CONFIG_DIR` | `<harness>/config` | the config directory, overriding `XDG_CONFIG_HOME` |
| `OPENCODE_SERVER_PASSWORD` | the app's secret | loopback API auth |

- **The harness home is `<TandemHome>/harness/`,** one directory that is copied and restored whole, with the database a neighbour of `tandem.db` in all but the last step:

```
~/.tandem/
  tandem.db
  harness/
    opencode.db        OPENCODE_DB
    config/            XDG_CONFIG_HOME and OPENCODE_CONFIG_DIR
    data/              XDG_DATA_HOME, including the model credentials
    state/             XDG_STATE_HOME
    cache/             XDG_CACHE_HOME
    tmp/               TMPDIR
    version            the version stamp written by the app
```

- **The version stamp guards the store.** The harness migrates its own database, so an older binary opening a newer store is the failure mode: before the first launch of a new harness version the app copies `opencode.db` to `opencode.db.<version>`, writes `harness/version`, and refuses to start when the stamp is newer than the bundled binary, offering the restore.
- **The model credentials move with the home.** `auth.json` is `XDG_DATA_HOME/opencode/auth.json`, so the harness starts unauthenticated and Tandem's sign-in surface writes there. The human's own `~/.local/share/opencode/auth.json` is neither read nor copied.
- **No app code in this ticket.** `app/` and `rust/` are untouched; the decision is recorded now because the launcher that spawns the harness is built next.

## The measurement

`Scripts/check-harness-home.py` runs the pinned binary and asserts seven things: nothing under HOME changed (mtime to the nanosecond, size, and the file set); the listener is loopback; a session was created and then listed over the API; the database exists at `OPENCODE_DB`; `XDG_DATA_HOME` took effect (the log path sits under it); the config root is inside the harness home; and closing stdin ends the run with exit 0. It also prints the paths it proved, which is how the table above was written.

The check seals HOME instead of aiming at the human's real one: it creates the three default directories empty under a scratch HOME, makes them read-only, and points the environment elsewhere. A write to a default path then fails loudly rather than landing in a live store, so the check is safe to run on a machine where another opencode process is working, which this machine always is. `--control` is the red half: the same pass without the environment against a writable sealed HOME. It changed 14 paths under the sealed HOME; with the environment, zero. The check trips where it should, which is the only thing that makes its green mean anything.

Two facts the spike measured that the brief got wrong, and the correction:

- **`--stdio` is not a stdio transport.** The binary still binds a loopback listener and serves the v2 API over HTTP; `--stdio` makes stdin the lifetime handle, nothing more. With no `--port` it takes 4096, which a developer's own opencode also wants, so the app passes `--port 0` and reads the port from the handshake. What the flag buys is a process that dies with its parent and a port nobody has to allocate; it does not buy "no listener", "no port" or "no password".
- **`OPENCODE_DB` and `OPENCODE_CONFIG_DIR` are the binary's `database.path` and `config.directory`.** The SDK option names do not exist on the command line, and without them the database would be `<data>/opencode/opencode.db`; with them it is the neighbour of `tandem.db` this ADR describes.

The one path XDG does not cover is the temp root: `os.tmpdir()/opencode`, plus the `.bun-*.dylib` and `.bun-*.node` files Bun extracts there, which `TMPDIR` inside the harness home catches. Bun writes them even on a run that does nothing but bootstrap the schema.

## Why

- **A child process, not the in-process SDK.** Already decided on size and signing (the SDK route needs Bun plus the package tree and a native sqlite module), and the spike adds the isolation argument: the environment seam is identical for both, and the binary needs no build step in the app's toolchain.
- **The environment, not a patched binary.** Patching `global.ts` or forking the build makes every upgrade a merge; the environment is read the same way in every version and is visible in the launcher's own source.
- **The database as a file beside `tandem.db`.** The restore and the migration copy are Tandem's acts, and one `OPENCODE_DB` puts the harness's store where Tandem's own backup and import can see it, instead of inside a vendor-shaped `data/opencode/` one directory deeper.
- **Spawning with a password rather than trusting loopback.** Any local process can reach a loopback port; the credential is one string the app already has to keep, and it costs nothing to require it.

## Decided against

- **A dedicated HOME for the harness.** It would isolate more than XDG does, and it would also lose the human's shell, git config, SSH agent and keychain: the harness runs tools in the human's name, and a fake HOME breaks that quietly. Sealing HOME survives only inside the check, where there are no tools to run.
- **A fixed port, or a port range.** It collides with the developer's own opencode and with a second Tandem window; `--port 0` plus the handshake has neither problem.
- **`OPENCODE_TEST_HOME`.** It overrides the `home` getter only, not the XDG roots, so it moves nothing this record needs moved.
- **Leaning on `app.name`.** Cosmetic for paths, and a future upstream change to it would look like it fixed this.
- **A sandbox profile (`sandbox-exec`) as the check.** Kernel-enforced, and deprecated, fiddly to keep green as the binary's syscall surface moves; the sealed-HOME ledger proves the same claim with a diff a human can read.

## Consequences

- **The check's proof is scoped to what it drives.** Session create and list, the config read and the schema bootstrap. A prompt, a tool call and a permission reply write under the same XDG data root (`tool-output`, `snapshot`, `worktree`), and the conformance test this ticket's upgrade section describes runs them; until it does, the deeper surfaces are reasoned, not measured.
- **The listener is a real surface on the app.** The main process owns the url and the password, and the renderer talks to the app, not to the harness; handing the renderer the credential would put the agent's API in the browser context and CORS back in play.
- **The harness home is 300 KB at first launch and grows with use** (the store was 299,008 bytes after one session), on top of the binary's 226,703,008 bytes in the bundle, per arch.
- **A failed spawn has no partial home to clean up:** `XDG_*` directories are created by the binary, so the app creates the home and the temp root before spawn and the rest is the binary's.
- **The check runs where the pinned binary is installed.** It is not wired into CI yet; the CI job that fetches and verifies the asset is where it belongs, and until then it is a local gate named in the code that spawns the harness.
- **Windows and Linux need their own pin, digest and Authenticode signature.** Nothing in this record transfers the macOS numbers.

## Deferred

- **The DMG delta.** 226,703,008 bytes per arch is the budget; what it costs the compressed artifact is not measured.
- **The restore UI and the stamp's refusal path.** The behaviour is decided above; the surface that offers the restore is the app's, and it follows the launcher.
- **The model auth screen, the approval surface and the per-provider fallback**, which the ticket this record comes from puts out of scope.
