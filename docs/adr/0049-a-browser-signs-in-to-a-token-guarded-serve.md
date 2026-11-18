# A browser signs in to a token-guarded serve: a session cookie, held by the instance

> **Status:** In force. See [the index](README.md).

[ADR 0047](0047-serve-is-the-single-door.md) put a bearer token on every route of a non-loopback `tandem serve` and deferred the browser half. A browser does not send `Authorization` on a page load, a form post or an `EventSource`, so a token-guarded board was reachable by agents and the remote CLI but not by a person. That blocks the self-hoster behind a reverse proxy, the container image (CUT-28) and the hosted product, whose fleet runs the unmodified release and keeps the control plane out of the request path ([ADR 0048](0048-hosted-tandem-is-one-serve-per-workspace.md)). The instance must therefore hold the browser session itself.

Decided 2026-11-16 (CUT-34).

## Decision

- **Token mode only.** Everything below applies when `TANDEM_TOKEN` is set. Loopback and `--allow-unauthenticated` behave as before, and `/login` and `/logout` answer `404` there: there is nothing to sign in to.
- **A login turns the token into a session.** `GET /login` is a plain form: a fixed `username` field a password manager can fill, the token as `type="password"`, a hidden same-origin return target. `POST /login` needs a present `Origin` that matches `Host` (the login CSRF stop), trims the token, and compares it on the same constant-time digest path as the bearer header. A right token mints a session and answers `303` to the return target with the cookie; a wrong one re-renders the page with one generic message. `POST /logout` deletes the server-side row, clears the cookie and lands on the login page.
- **The cookie.** `__Host-tandem_session`: `HttpOnly`, `Secure`, `SameSite=Lax`, `Path=/`, no `Domain`, `Max-Age` 30 days. `Lax` rather than `Strict` so a link from mail or chat opens the board signed in; cross-site writes are stopped by `Lax` withholding the cookie on cross-site posts and by the `Origin` rule below. Because the cookie is `Secure`, the browser must reach the server over HTTPS or on localhost.
- **The session id** is 32 bytes from the operating system's generator, minted at login only and never taken from the client; a login replaces any session the browser presents.
- **At rest, a keyed digest.** The store keeps the SHA-256 of the id keyed with the in-memory SHA-256 of the token, never the id. A changed `TANDEM_TOKEN` therefore matches no stored row: rotating the token signs every browser out. Nothing derived from the token alone reaches disk or `tandem.db.bak`.
- **The table.** `browser_sessions` (digest, created, last used), schema v13 ([ADR 0031](0031-one-database-the-store.md)'s migration rules; additive, so an older binary refuses the home afterwards, as with every migration). Not `sessions`: that name was a retired table dropped at v11. No member or owner column: CUT-36 adds one in its own migration. All SQL lives under `rust/core/src/store/` ([ADR 0009](0009-cli-agent-face.md)).
- **A session row is not board data.** Session writes (create, touch, delete, sweep) go through their own store path: they migrate lazily like any write, but never move the board revision and never arm the rolling backup ([ADR 0018](0018-rolling-store-backup.md)), so signing in or browsing wakes no `/events` stream. They are outside the MCP tools. `tandem.db.bak` is a whole-file copy and does carry the digests; they are keyed, so they are useless without the token. The export of CUT-35 leaves session rows out (a pointer, not built here).
- **Lifetime.** 30 days since last use, checked on every lookup. Last use is rewritten at most once a day per session and the cookie's `Max-Age` is refreshed with it. An open `/events` stream counts as use only when it connects. Expired rows are swept at startup and at each login. Sessions survive a restart.
- **The guard accepts the cookie where it accepts the bearer header,** except `/mcp`, which stays bearer only and ignores the cookie. `/health` stays open. `/login` and `GET /static/*` are exempt from the token by name; nothing else is. When the cookie is what authenticates a write, `Origin` must be present and match `Host` (a bearer write keeps today's rule). The lookup is a `Store` method run off the async thread; any failure (a locked store, a missing table on a store not yet migrated) is signed out, fail closed.
- **Signed out.** A `GET` page load answers `303` to `/login` carrying a same-origin return path (one leading `/`, the second character neither `/` nor a backslash, no control characters, else `/`); every other request (`/events`, form posts, htmx requests) answers `401`. An open `/events` stream ends once its session does.
- **Throttle.** A wrong token is answered after a fixed delay; at most a few wait at once and the excess gets `429`. Global, not per address: the peer is the proxy and `X-Forwarded-For` is untrusted until the public origin setting (CUT-37). A right token is never delayed or refused.
- **An open tab whose session ends** shows an in-place "You are signed out" banner with a link to `/login`, on a `401` from any background request and on a refused event stream. It does not redirect or reload, so an unsaved note or comment stays on screen.
- **Never logged or printed:** the token, the form body, the cookie, the id, the digest.

## Supersedes

By quotation, the browser-login half of [ADR 0047](0047-serve-is-the-single-door.md), decision bullet "Off loopback, a stated choice, and real authentication deferred":

- "every route but `/health` wants `Authorization: Bearer <token>`": now every route but `/health`, `/login` and `GET /static/*` wants the bearer header or, except on `/mcp`, a live session cookie.
- "never stored": the token is still never stored; a keyed digest of a session id is.
- "Real authentication (per-actor tokens, a browser login) is a later ticket": the browser login is this record. Per-actor tokens stay deferred to CUT-36.

## Why

- **The instance holds the session.** A signed assertion or a proxy that holds the session would put a service in the request path. A row in the instance's own home keeps the hosted fleet on the unmodified release.
- **A stored fingerprint, not a stateless cookie.** Sign out and CUT-36's member removal need something to delete, and the human chose sessions that survive a restart.

## Rejected

- A one-time login URL printed at startup (a secret in history, proxy logs and scrollback; HOS-16 owns a redirect-style sign-in), `SameSite=Strict`, a separate form token for CSRF, a stateless signed cookie or memory-only sessions, the cookie on `/mcp`, a per-address throttle or lockout, a redirect or reload on a background `401`.
