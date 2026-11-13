## What

Merges the cut's Phase 3 and Phase 4 into one server-rendered board on `tandem serve`, and retires `tandem rpc`.

- **ADR 0046** records the merged decision: one binary, `tandem serve` on `127.0.0.1:7700` (`axum` + `tokio`), `SessionContext { store, actor, environment }` as the router's default state, Askama templates with vendored htmx, a loopback-only bind, and standalone binary distribution. ADRs 0003, 0036 and 0037 carry dated superseded banners.
- **`tandem-core::serve`** adds the in-process router and the process seam. Routes: `GET /` and `GET /board` (project focus, show-all, lane, assignee, accent-folded title search), `POST /tickets` (revision-pinned add), `POST /tickets/{id}/status` (revision-pinned drop, same-column no-op), `GET /events` (SSE over the existing 40 ms store watcher), and `GET /static/{*path}` for the embedded assets.
- **Guards**: mutating POSTs require `application/x-www-form-urlencoded`, refuse a cross-origin `Origin`, and are capped at `rpc::MAX_FRAME`; every HTML response carries the CSP. Askama escapes ticket titles.
- **Templates and assets** live in `rust/core/templates/` and `rust/core/src/serve/static/`, including the copied `tokens.css`/`app.css`, vendored htmx 2.0.8, its SSE extension, and `tandem.js` for the card drop.
- **CLI**: `tandem serve` with `--host`, `--port`, `--open`, `--allow-unauthenticated`; a non-loopback host exits 2 before binding and `--port 0` prints the bound port. `Command::Rpc`, `fn rpc` and `usage::RPC` are removed; `rpc::handle_in`, `rpc::Stream` and `rpc::StdioStream` stay for `tandem mcp`.
- **Tests**: `rust/cli/tests/rpc.rs` is replaced by `rust/cli/tests/serve.rs` (7 process cases), and `rust/core/src/serve/` adds the 10 router and watcher cases named in the brief. The watch's old rpc unit cases moved with the watcher.

## Verification

Run in `rust/` on the pushed head:

- `cargo fmt --all --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `cargo test -p tandem-core --lib serve::tests`
- `cargo test -p tandem --test serve`
- `cargo test -p tandem --test mcp`

No U+2014 in any added line (`git diff` added-line scan is empty).

## Merge Danger

**Door:** one-way. This retires a user-facing door (`tandem rpc`), commits the UI transport to the loopback server, and supersedes the distribution records. A revert restores the stdio transport and the Electron-era assumptions, so it is a deliberate rollback rather than a cheap flip.

**Blast Radius:** high. New runtime dependencies (`axum`, `askama`, `tokio`, `tokio-stream`), a new CLI verb, a removed CLI verb, a new HTTP surface, and the board's write paths now go through revision-pinned forms. The app tree is untouched (CUT-15 owns its removal), so the app job still exercises the old renderer against the unchanged core wire.

Base is `cut/simplification`, the cut's integration line; `main` is untouched.
