# Contributing to Tandem

Thanks for stopping by. Tandem is an AGPL-3.0-licensed board for you and your
AI coding agents that ships as one standalone binary (`tandem serve` plus the
CLI), and contributions are welcome.
This file is the on-ramp: what
you need, how to set the repo up, which checks your pull request must pass, and
the conventions the project keeps. The tracker is the Tandem board, not GitHub issues.

## Prerequisites

Contributing needs only a checkout on any of the three CI platforms below; an
Apple silicon Mac or x86_64 Linux machine is only for shipped-binary work.

- Any of the three CI platforms (macOS, Windows, Ubuntu) for Rust or release
  script work. CI runs the cargo gates on all three, and the installer suite
  plus the archived-binary HTTP smoke test on macOS and Linux, so a checkout
  there builds and tests what it can run. Every tag publishes five archives:
  macOS arm64 and x86_64, static musl Linux x86_64 and arm64, and Windows x86_64.
- Only for shipped-binary work: an Apple silicon Mac or an x86_64 Linux
  machine, for running the installer and the binary the release publishes.
  `install.sh` verifies the archive's digest and installs into
  `~/.local/bin`; on macOS it strips the quarantine flag. `install.ps1` is its
  Windows twin (user PATH, `%LOCALAPPDATA%\Programs\tandem`); its suite,
  `Scripts/test_install.ps1`, runs under Windows PowerShell 5.1 in the
  Windows build job of `release.yml` against the zip that job just packaged.
- Rust via [rustup](https://rustup.rs). The pin lives in
  [rust/rust-toolchain.toml](rust/rust-toolchain.toml) (currently 1.99.1 with
  the `rustfmt` and `clippy` components), so the right toolchain arrives on
  first use; do not override it with a floating channel.

## Setup

Clone the repo and install each area you will touch:

```sh
git clone https://github.com/gianniacquisto/tandem.git
cd tandem
```

Rust needs nothing beyond the toolchain above: `cargo` fetches the rest on
first build. The served faces ship with the binary; there is no second
toolchain to install.

## The gates

Every pull request runs the same checks CI runs. Run them locally before you
push, in each area your change touches. All three are required for every PR.

In `rust/`:

```sh
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

## Browser tests

The board's own JavaScript (`rust/core/src/serve/static/tandem.js`) is covered by a Playwright suite in `e2e/`, a separate Node package so the Rust workspace gains no Node dependency (ADR 0046, amendment of 2026-11-25). It runs a real `tandem serve` on a scratch home and a free port, in Chromium, Firefox and WebKit.

Run the whole suite with one command from the repo root:

```sh
(cd rust && cargo build -p tandem) && cd e2e && npm ci && npx playwright install chromium firefox webkit && npm test
```

After the first run, `cd e2e && npm test` is enough once the binary is built (`TANDEM_BIN` points the suite at another binary). The HTML report lands in `e2e/playwright-report/`, and each case's screenshot and trace in `e2e/test-results/`; open a trace with `npx playwright show-trace <trace.zip>`.

CI runs the same suite in the `e2e` job on every pull request that touches the static assets, the templates, Rust, `e2e/` or the workflow, and uploads the report, screenshots and traces as the `playwright-report` artifact, pass or fail. That artifact is where a UI change's proof lives.

To add a case for your own UI change, add a `test(...)` to `e2e/tests/board.spec.js`: call `startServer()` from `e2e/tests/harness.js` (it seeds one project, label, ticket and note, and returns `baseURL`, `cli`, `readNote` and `writeNote`), drive the page, assert on the page and on the store, attach a screenshot of the end state with `testInfo.attach('end-state', ...)`, and `await srv.stop()` in a `finally`. Keep the case to one behaviour.

## Run from source

To try a change in the real board, build the `tandem` binary and serve a
throwaway home. The installed release binary in `~/.local/bin` is not your
source build: run the one under `rust/target/` by its path.

In `rust/`:

```sh
cargo build -p tandem
TANDEM_HOME="$(mktemp -d)" ./target/debug/tandem serve --open
```

This binds loopback (`127.0.0.1`) on port 7700 and opens the board in your
browser. Pass `--port <n>` to use another port, for example when an installed
`tandem serve` already holds 7700. A fresh `TANDEM_HOME` keeps your real
`~/.tandem` out of the test.

### Team mode from source

The team features (members, sign-in keys, the People screen) need a little
more setup. A home enters team mode when its first member is added, and a
browser signs in only in token mode, which applies only to a non-loopback
bind: on loopback `TANDEM_TOKEN` is ignored.

**A non-loopback bind is reachable from your network.** Use a throwaway home
and a throwaway port, and stop the server when you are done.

```sh
export TANDEM_TOKEN='<owner token, at least 32 characters>'
TANDEM_HOME="$(mktemp -d)" ./target/debug/tandem serve --host 0.0.0.0 --port 7799
```

`--host` also takes a LAN address. Either way, open the board at
`http://localhost:7799`, not at the LAN address: the session cookie is
`Secure`, so the browser keeps it only on localhost or over HTTPS. The login
form also refuses a page loaded from another origin ("the request came from
another origin"), so load it from `localhost` or `127.0.0.1` on the serve's
port.

Bootstrap the first admin as the owner: sign in at the login page with
`TANDEM_TOKEN`, open `http://localhost:7799/people` by typing it (the menu
does not link it before the first person exists), and add a person with the
role admin. The next page shows their sign-in key once: sign out and paste it
into the login page to use the board as that person. Add more people the same
way. The full walk-through, with the credentials and the failure symptoms, is
[docs/self-hosting.md](docs/self-hosting.md).

To test the operator interface instead, start the serve with
`TANDEM_OPERATOR_KEY` set too (at least 32 characters, not the token) and add
the person with it ([docs/operator.md](docs/operator.md)). The serve speaks
plain HTTP, so use `http://`:

```sh
curl -X POST http://localhost:7799/operator/v1/members \
  -H "Authorization: Bearer $TANDEM_OPERATOR_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name": "ada", "role": "admin"}'
```

The operator issues no sign-in key, so the person cannot sign in yet: sign in
as the owner, open `/people` and use Reissue sign-in key on their row.

## Conventions

- Branch off `main`, and target pull requests at `main`. `main` is the trunk:
  it takes merges, and a release is a lightweight tag on its head.
- Prefer merge commits over rebases, and avoid force pushes and other
  destructive git actions unless absolutely necessary. When a force push is
  unavoidable, use `--force-with-lease`, never bare `--force`.
- Use the project's words: [GLOSSARY.md](GLOSSARY.md) is the vocabulary.
- Scripts live in `Scripts/`: the release, gate and fixture tooling. There is
  no JavaScript build step.
- The served faces are Askama templates and static assets under
  `rust/core/templates/` and `rust/core/src/serve/static/`; `tandem serve` is
  the one door.
- Respect the decisions in [docs/adr/](docs/adr/), and amend the ADR in the
  same pull request when you change what it decided.
- The tracker is the Tandem board, not GitHub issues: the tracker contract is
  the starter instructions ([examples/AGENTS.md](examples/AGENTS.md)). A
  ticket's description is written once as the brief and appended to with the
  outcome, never overwritten; the brief's shape is the refiner's
  ([examples/agents/refiner.md](examples/agents/refiner.md)).

## Releases

Releases are maintainer-cut from a lightweight tag on `main`'s head, per
[docs/releasing.md](docs/releasing.md). Contributors do not cut releases; land
your change on `main` and the maintainer takes it from there.

## Contributor agreement

Contributions are made under the [Contributor License Agreement](CLA.md), the
Apache Individual CLA adapted to this project. Ticking the CLA checkbox in your
pull request description accepts it for that pull request and your later ones,
and it is required for a merge: a pull request from an outside author without
the box ticked is not merged. The maintainer and the agents working under the
maintainer's account do not need to tick it, and any other co-maintainer does.

## License

The core is AGPL-3.0-only, see [LICENSE](LICENSE); `examples/` is MIT, see
[examples/LICENSE](examples/LICENSE). Releases up to v2.9.1 stay MIT. The third-party notices are in
[NOTICE.txt](NOTICE.txt).
