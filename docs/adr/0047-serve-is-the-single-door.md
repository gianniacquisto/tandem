# Tandem serve is the single door: MCP over HTTP, one process

> **Status:** In force, amended (CUT-37). See [the index](README.md).

Every agent session spawned its own `tandem mcp` child over stdio, each opening `tandem.db`, so the human saw a row of `tandem` processes and no single entry point, and no agent could reach a board on another machine. One long-lived `tandem serve` now answers the browser, the agents and a remote CLI over one protocol.

Decided 2026-11-15 (CUT-29, which absorbed CUT-20).

> **Amended by [ADR 0049](0049-a-browser-signs-in-to-a-token-guarded-serve.md):** the browser-login half of the off-loopback bullet is superseded. A token-mode browser signs in with a session cookie; `/mcp` stays bearer only.

## Decision

- **`serve` is the single door.** `tandem serve` answers `POST /mcp` (MCP streamable HTTP, JSON responses only, stateless: no `Mcp-Session-Id`, `GET` and `DELETE` answer `405`) and `GET /health` (the fixed body `ok`, no auth, no version, path, actor or store state). The handler passes the body to `mcp::handle_mcp` once, and a notification answers `202`. No JSON-RPC is parsed in `serve`: one dispatch (ADR 0009). The dispatch runs under `spawn_blocking`, because every session now shares one runtime.
- **Stdio retires.** The `mcp` verb, `mcp::serve`, `rpc::Stream` and `rpc::StdioStream` are deleted. `tandem mcp` reads as an unknown verb. Serve is started by hand; nothing platform-specific ships (no service file, no launchd).
- **One guard layer, two policies chosen by route.** `/mcp` and `/health` sit in the one route table under the one layer; no route opts out. On a loopback bind every route requires `Host` equal, case-insensitively, to `127.0.0.1:PORT`, `localhost:PORT` or `[::1]:PORT` at the port actually bound (the DNS-rebinding stop, which comparing `Origin` to the request's own `Host` cannot be). `/mcp` requires `Origin` absent or exactly `http://` and one of those authorities, and a `Content-Type` whose media type is `application/json`; form routes keep the form policy, and no UI route accepts JSON. No CORS header is ever answered.
- **The actor is per request.** `X-Tandem-Agent` names the actor, resolved in `ResolveContext::resolve` and nowhere else: 1 to 64 characters of lowercase ASCII letters, digits and hyphen, else `400`; `human` is allowed. Order: the header, else the server's own actor (`TANDEM_AGENT` of the serve process, else `human`). No request header reaches `Environment`. On loopback any local process can claim any name, as it can with `TANDEM_AGENT` today; the header is attribution, not authorization.
- **Off loopback, a stated choice, and real authentication deferred (human decision, 2026-11-15).** The `--allow-unauthenticated` flag stays: a non-loopback `--host` is refused with exit 2 before binding unless the flag is passed, so a phone on a tailnet reaches the board and `/mcp` with no token. An opted-in bind has no Host check and no token (the network is the boundary), and still applies the Origin and Content-Type checks. A token is the other way in: with `TANDEM_TOKEN` in the serve process's environment (at least 32 characters, else exit 2; never a flag value, never stored), every route but `/health` wants `Authorization: Bearer <token>`, compared in constant time over fixed-length digests with one `401` path for missing, malformed and wrong, and a TLS terminator must front the server. One token is one trust domain, so `X-Tandem-Agent` still names the actor on a token request. Real authentication (per-actor tokens, a browser login) is a later ticket.
- **The remote CLI.** With `TANDEM_SERVER` (a URL) set, `tandem-cli` sends each verb as an MCP `tools/call` to `<url>/mcp`, with `TANDEM_TOKEN` as the bearer when set, and prints the stdout JSON and exit codes local mode prints. The verb-to-tool table is the core's (`mcp::remote_verb`, over `wire::rpc::METHODS`). A verb with no tool (`serve`, `note rename`, `note stats`, `ticket set --append-description`) refuses in remote mode by name. [Amended by ADR 0050: `export` and `import` join that list; both open the home directly and refuse in remote mode by name.] [Amended by MCP-4: `ticket set --append-description` leaves that list; `ticket_set` carries `appendDescription`, so the verb runs remotely.] The CLI refuses to send a token to an `http://` URL whose host is not loopback, follows no redirect and never echoes the token. Unset `TANDEM_SERVER` keeps the direct store path.
- **The stale-writer bound moves to the server's uptime.** ADR 0009 bounded a stale MCP writer by the session's length. A long-lived `serve` is the shape ADR 0009's first ground names, arbitrarily old while holding write access. What holds it: the schema seam refuses a newer store, so an upgrade past the server's schema stops its writes instead of corrupting; restart after an upgrade.

## Supersedes

By quotation, in the same PR:

- ADR 0046, decision bullet 1, the `--allow-unauthenticated` flag text, is kept in substance (the flag stays) and extended by the token above.
- ADR 0046: "`serve` and `mcp` are the two server doors." Now `serve` is the one.
- ADR 0046: "No HTTP JSON-RPC. `POST /api/rpc` is not exposed ... `rpc::handle_in`, `rpc::Stream` and `rpc::StdioStream` stay for `tandem mcp`". `POST /mcp` is the MCP transport only; `/api/rpc` stays absent, and the board keeps HTML routes and SSE. The stream types are deleted.
- ADR 0046, Why: "Loopback is the authentication." On loopback the Host, Origin and Content-Type checks are the guard; off loopback the network or the token is.
- The port plan (`port.md`, since removed from `docs/`): "No daemon". A long-running `serve` is the daemon, started by hand.

## Why

- **One process, one protocol.** The browser, every agent session and a remote CLI speak to the same server, and `tandem.db` has one long-lived writer beside the CLI's short ones.
- **Loopback alone is not safe against a browser.** DNS rebinding and a cross-origin POST reach `127.0.0.1`; an exact Host check and a route-keyed Origin and Content-Type policy stop both.
- **A phone is the reason the flag stays.** The human reaches serve over Tailscale and wants no token to manage; the cost is that anything on that network can write, which the docs and the refusal text say.

## Consequences

- Sessions get no tools while serve is down. Start it by hand.
- Release notes name the retired verb `tandem mcp` and the replacement config: Claude Code `{"type": "http", "url": "http://127.0.0.1:7700/mcp", "headers": {"X-Tandem-Agent": "<role>"}}`, opencode `{"type": "remote", "url": "http://127.0.0.1:7700/mcp", "headers": {"X-Tandem-Agent": "<role>"}, "oauth": false}`.
- TAN-336 narrows: an agent's session can name itself through the header; the harness's own subagent dispatch still sets no `TANDEM_AGENT` for the CLI.

## Amended by CUT-37

By quotation, in the same PR:

- "`/mcp` requires `Origin` absent or exactly `http://` and one of those authorities": also accepted, when one is configured, is that one public `https` origin, compared as exact string equality against a value normalized once at startup. Form writes then require `Origin` to equal it, in place of the compare against `Host`. No other origin widens, and a request with more than one `Origin` is refused.
- "a TLS terminator must front the server": the origin a browser then sends is a setting, never derived from forwarded headers, which are not read. `Host` stays unchecked in token and open mode, and no CORS header is answered. The setting is refused on a loopback bind. How to set it: `docs/reverse-proxy.md`.

