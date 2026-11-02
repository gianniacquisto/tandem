# Contributing to Tandem

Thanks for stopping by. Tandem is an MIT-licensed macOS board for you and your
AI coding agents, and contributions are welcome. This file is the on-ramp: what
you need, how to set the repo up, which checks your pull request must pass, and
the conventions the project keeps. The short version lives in the README's
[How to contribute](README.md#how-to-contribute); this file is the full one.

## Prerequisites

- Any of the three CI platforms (macOS, Windows, Ubuntu) for Rust or app code,
  test and packaging work. CI runs the Rust suite and the app suite
  (typecheck, tests, packaging, artifact check, and smoke on macOS and Linux
  only) on all three, so a
  checkout there builds and tests what it can run. The packaged app is
  verified on all three by CI, but only the macOS artifact ships: Windows and
  Linux app builds are unshipped and unsupported.
- An Apple silicon Mac running macOS 27 or later, for anything that touches
  the shipped app: installing the cask or the script, the Gatekeeper strip,
  running the built `Tandem.app`. The release publishes macOS artifacts only,
  and the scripted install is macOS-only (it refuses off Darwin/arm64 and
  writes to `/Applications`).
- Rust via [rustup](https://rustup.rs). The pin lives in
  [rust/rust-toolchain.toml](rust/rust-toolchain.toml) (currently 1.99.1 with
  the `rustfmt` and `clippy` components), so the right toolchain arrives on
  first use; do not override it with a floating channel.
- Node 22, for `app/` and `Editor/`.

## Setup

Clone the repo and install each area you will touch:

```sh
git clone https://github.com/gianniacquisto/tandem.git
cd tandem
```

Rust needs nothing beyond the toolchain above: `cargo` fetches the rest on
first build. The two JavaScript areas install from their lockfiles:

```sh
cd app && npm ci
cd ../Editor && npm ci
```

## The gates

Every pull request runs the same checks CI runs. Run them locally before you
push, in each area your change touches. All three are required for every PR.

In `rust/`:

```sh
cargo test
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

In `app/`:

```sh
npm run typecheck && npm test
```

In `Editor/`:

```sh
npm ci && node --test
```

Note on the editor bundle: the committed editor bundle is prebuilt, and CI
rebuilds it and fails on any byte that differs. If you touch `Editor/src`,
rebuild the bundle and commit the result, so what you tested is byte-identical
to what CI builds.

## Conventions

- Branch off `main`, and target pull requests at `main`. `main` is the trunk:
  it takes merges, and a release is a lightweight tag on its head.
- Prefer merge commits over rebases, and avoid force pushes and other
  destructive git actions unless absolutely necessary. When a force push is
  unavoidable, use `--force-with-lease`, never bare `--force`.
- Use the project's words: [GLOSSARY.md](GLOSSARY.md) is the vocabulary.
- Respect the decisions in [docs/adr/](docs/adr/), and amend the ADR in the
  same pull request when you change what it decided.
- The tracker is the Tandem board, not GitHub issues: the tracker contract is
  the `tandem` skill ([skills/tandem/SKILL.md](skills/tandem/SKILL.md)). A
  ticket's description is written once as the brief and appended to with the
  outcome, never overwritten; the brief's shape is the refiner's
  ([agents/refiner.md](agents/refiner.md)).

## Releases

Releases are maintainer-cut from a lightweight tag on `main`'s head, per
[docs/releasing.md](docs/releasing.md). Contributors do not cut releases; land
your change on `main` and the maintainer takes it from there.

## License

Tandem is MIT licensed (copyright 2025 Gianni Acquisto); see
[LICENSE](LICENSE). The third-party notices are in
[NOTICE.txt](NOTICE.txt). There is no contributor license agreement and no
Developer Certificate of Origin: your pull request under the MIT license is
enough.
