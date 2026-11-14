# Contributing to Tandem

Thanks for stopping by. Tandem is an MIT-licensed board for you and your
AI coding agents that ships as a macOS app, and contributions are welcome.
This file is the on-ramp: what
you need, how to set the repo up, which checks your pull request must pass, and
the conventions the project keeps. The tracker is the Tandem board, not GitHub issues.

## Prerequisites

Contributing needs only a checkout on any of the three CI platforms below; an
Apple silicon Mac is only for shipped-app work.

- Any of the three CI platforms (macOS, Windows, Ubuntu) for Rust or release
  script work. CI runs the cargo gates on all three, and the installer suite
  plus the archived-binary HTTP smoke test on macOS and Linux, so a checkout
  there builds and tests what it can run. Every tag publishes macOS arm64,
  Linux x86_64 and Windows x86_64 archives.
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
  the `tandem` skill ([examples/skills/tandem/SKILL.md](examples/skills/tandem/SKILL.md)). A
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
