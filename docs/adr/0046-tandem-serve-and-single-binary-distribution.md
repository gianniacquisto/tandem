# Tandem serve: one binary, one loopback server, server-rendered HTML

> **Status:** In force, amended by 0047 (single door). See [the index](README.md).

Tandem's Rust core already owns the domain, the store and the wire, but the UI still needs a second stack to draw it: Electron spawns the core over stdio and a React renderer talks JSON-RPC back. The next step is one binary. Keeping `POST /api/rpc` alive for the React face would preserve a transport that a server-rendered board replaces in the same release, so the cut merges the transport phase and the HTML phase into one decision.

Decided 2026-11-13 (CUT-14).

## Decision

- **`tandem serve` is the UI's door.** The verb binds `127.0.0.1:7700` by default with `--host`, `--port`, `--open` and `--allow-unauthenticated` flags, prints the address it actually bound (so `--port 0` is usable), and ends on SIGINT or SIGTERM. A non-loopback host is refused with exit 2 before binding unless `--allow-unauthenticated` is set.
- **One context is the state.** `SessionContext { store, actor, environment }` lives in `tandem-core` and is injected as the axum router's default state, so every route reads and writes through the same store and actor the CLI uses.
- **The board is server-rendered HTML.** Askama templates are compiled into `tandem-core`; htmx swaps the `#board-root` partial. The routes are `GET /` and `GET /board` (project focus, show-all, lane, assignee, title search), `POST /tickets`, `POST /tickets/{id}/status`, `GET /events` (SSE over the store watcher), and `GET /static/{*path}` (embedded CSS, the vendored htmx and SSE extension, and a small `tandem.js` for the card drop).
- **The write guards are in the server.** Every mutating POST requires `application/x-www-form-urlencoded`, refuses a cross-origin `Origin`, and caps its body at `rpc::MAX_FRAME` (4 MiB). Every HTML response carries a `Content-Security-Policy` that allows only same-origin scripts and connections.
- **No HTTP JSON-RPC.** `POST /api/rpc` is not exposed: the UI speaks HTML routes and SSE, and no HTTP JSON-RPC client exists. `rpc::handle_in`, `rpc::Stream` and `rpc::StdioStream` stay for `tandem mcp`, which is the one caller left.
- **`tandem rpc` is retired.** The stdio door the Electron shell spawned goes with the shell; `serve` and `mcp` are the two server doors.
- **Electron, React, Vite, vitest, `ts-rs` and `app/` retire.** A later cut deletes the renderer tree; this record writes the replacement it answers to.
- **Distribution is one standalone binary.** Releases publish archive files per platform with `SHA256SUMS` and an `install.sh`; five targets (amended 2026-11-21, CUT-22): darwin-arm64, darwin-x86_64 (cross-built on the arm64 runner), windows-x86_64, and static musl linux-x86_64 and linux-arm64, so one Linux archive per architecture runs on any distro, glibc or not (no separate glibc variant); the Homebrew tap and `Scripts/bump-tap.sh` retire.

## Amendment (2026-11-14)

Decision bullet 1 gains application logging: `tandem serve` writes its log to stdout after the listening line, which stays the first stdout line and keeps its text. `TANDEM_LOG` sets the filter in RUST_LOG syntax (default `info`), and the log covers `tandem serve` and `tandem mcp`, so ADR 0009's split for the `ticket`, `lane`, `note` and `doctor` verbs, which stay unlogged, is unchanged. What a line may carry is the serve brief's never-logged rule: identifiers and escaped values, never request bodies, ticket or note text, or the environment.

`tandem mcp` (amended 2026-11-15) logs too, on stderr, because its stdout carries JSON-RPC frames and a log line there would corrupt the protocol. It shares the installer and the `TANDEM_LOG` variable, with the same invalid-filter behaviour (one notice, the value never echoed), but its default filter is `warn`, so a healthy session writes nothing to stderr and a host that captures stderr for a whole session sees no per-call noise. Its `info` events (start, `initialize`, each `tools/call` with tool name, outcome, error kind and latency, stdin closed) and `debug` events (`ping`, notifications, `tools/list`) appear on request; parse failures, oversize or non-UTF-8 frames, invalid requests and unknown methods are `warn`. The never-logged rule holds: no arguments, results or error messages, and client-chosen names (tool, method) are Debug-escaped.

## Amendment (2026-11-15)

[ADR 0047](0047-serve-is-the-single-door.md) supersedes "`serve` and `mcp` are the two server doors", "No HTTP JSON-RPC" (and its `rpc::Stream` clause) and "Loopback is the authentication". `--allow-unauthenticated` stays, beside an optional `TANDEM_TOKEN`.

## Why

- **A transport for a face that is leaving is a migration twice.** `POST /api/rpc` plus Vite asset serving would have been written, tested and deleted inside one release. The board routes are the first real client, so they are the first transport.
- **HTML removes the bundle step.** Askama and htmx compile into the binary, so there is no Node toolchain, no asset manifest and no second process to keep in step with the core.
- **Loopback is the authentication.** The server binds the machine's own interface and refuses a non-loopback address unless the human says otherwise, so an untrusted page cannot reach the store through a browser POST.
- **One binary is the install.** A macOS cask wrapped an Electron bundle whose only Rust content was the core. A standalone archive is the same core with an HTML face and no app bundle.

## Consequences

- The board's writes are revision-pinned: a form carries the revision it was drawn from, and a stale write re-renders with a dismissible conflict banner instead of overwriting another writer.
- `app/` remains in the tree until the next cut deletes it; `serve` does not depend on it, and the CSS it serves is copied into `tandem-core` so the binary has no build-time renderer dependency.
- Pictures and scripts that spawned `tandem rpc` read an unknown verb; the release notes name the removal, and the MCP door is unchanged for agents.
- The embedded asset table is deliberately small. A new static file is one entry in the handler and one `include_bytes!`, not a build pipeline.
