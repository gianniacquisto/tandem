# Contributing to Tandem

Thanks for stopping by. Tandem is an MIT-licensed board for you and your
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
  `~/.local/bin`; on macOS it strips the quarantine flag.
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

## License

Tandem is MIT licensed (copyright 2026 Gianni Acquisto); see
[LICENSE](LICENSE). The third-party notices are in
[NOTICE.txt](NOTICE.txt). There is no contributor license agreement and no
Developer Certificate of Origin: your pull request under the MIT license is
enough.
