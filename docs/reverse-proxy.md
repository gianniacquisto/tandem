# Serving behind a reverse proxy

`tandem serve` can sit behind a TLS proxy on a public name, such as `https://workspace.example`. This page is the one home for that setup; the flag text is in `tandem-cli serve --help`.

## Topology

- TLS ends at the proxy. Tandem listens on a non-loopback `--host` (for example `0.0.0.0`) behind it.
- `TANDEM_TOKEN` is set to 32 or more characters. Every route but `/health` then wants `Authorization: Bearer <token>`.
- Never use `--allow-unauthenticated` on a public name: it adds no authentication.

## The setting

Tell the server the one origin a browser sees, with `--public-origin <origin>` or `TANDEM_PUBLIC_ORIGIN` (the flag wins; an empty variable is unset; an empty flag is an error):

```
TANDEM_TOKEN=... tandem serve --host 0.0.0.0 --public-origin https://workspace.example
```

- Right: `https://workspace.example`, `https://workspace.example:8443`.
- Wrong: `https://workspace.example/` (a browser never sends a trailing slash, so every write would be refused), `http://workspace.example` (a token over cleartext is not supported), a path, query or userinfo.

A malformed value, or any value on a loopback `--host`, stops startup with exit 2. The server prints the origin it holds on its own line after the `listening on` line. A restart applies a changed value.

With it set, a browser form write and `/mcp` accept an `Origin` exactly equal to that value and no other. Forwarded headers (`X-Forwarded-*`, `Forwarded`) are never read, and no CORS header is ever sent. `Host` is not checked in token mode, so a proxy that rewrites it works.

## What the proxy must do

- Forward `Authorization`.
- Stream `/events` unbuffered (server-sent events), with a long read timeout.
- `Host` may be forwarded or rewritten once the origin is set.

## Symptoms

| You see | Meaning |
|---|---|
| Browser writes answer `403` "the request came from another origin" while reads work | The configured origin differs from the address bar's origin, or none is set and the proxy rewrites `Host`. |
| `401` | The token, not the origin. |
| Agents and the CLI work, the browser does not | Agents and the CLI send no `Origin` and need no setting. |

The board logs no presented value, so compare the setting against the address bar. The log line says which check refused (form or `/mcp`) and whether an origin is configured.

## Notes

- The remote CLI must use the https public name: it refuses to send `TANDEM_TOKEN` to a non-loopback `http://` URL.
- In `--allow-unauthenticated` mode the setting is accepted, but a direct-address browser write (a phone on a tailnet) no longer matches and is refused.
- Rule record: [ADR 0047, serve is the single door](adr/0047-serve-is-the-single-door.md).
