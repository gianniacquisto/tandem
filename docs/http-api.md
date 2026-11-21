# HTTP API reference

`tandem serve` answers HTTP on one port (default `127.0.0.1:7700`). This page lists the routes meant for programs: health checks, the MCP endpoint, the live change stream, the export and the operator API. The other routes (`/`, `/board`, `/tickets/...`, `/notes/...`, `/people`, `/tokens`) draw and drive the board in the browser. Their shape may change in any release, so do not script against them: use [MCP](mcp.md) or the [CLI](cli.md) instead.

| Route | Method | For | Reference |
| --- | --- | --- | --- |
| `/health` | `GET` | Liveness checks | [below](#get-health) |
| `/mcp` | `POST` | Agents and the remote CLI | [MCP reference](mcp.md) |
| `/events` | `GET` | Following changes live | [below](#get-events) |
| `/export` | `GET` | Downloading the whole home | [below](#get-export) |
| `/operator/v1/...` | various | A hosting service managing members | [Operator API](operator.md) |

## Authentication

What a request needs depends on how the server was started ([configuration](configuration.md#serving-to-a-network)):

| Server mode | Started with | A request needs |
| --- | --- | --- |
| Loopback | the default `--host 127.0.0.1` | Nothing. The machine is trusted. |
| Open | a non-loopback `--host` and `--allow-unauthenticated` | Nothing. Only for a network you trust. |
| Token | a non-loopback `--host` and `TANDEM_TOKEN` | `Authorization: Bearer <credential>`, or a browser session cookie from `/login`, on every route but `/health`, `/login` and the static assets. |

In token mode, the credential decides what the request may do. The owner token (`TANDEM_TOKEN`) may do everything. In [team mode](self-hosting.md), a member signs in a browser with a sign-in key (`tdk_...`), and gives their agent a personal token (`tdm_...`):

| Credential | Board and MCP | Export | Manage people | Own tokens | Access record |
| --- | --- | --- | --- | --- | --- |
| Owner token | yes | yes | yes | yes | yes |
| Admin's browser session | yes | yes | yes | yes | yes |
| Member's browser session | yes | no | no | yes | no |
| Personal token, admin or member | `/mcp` and `/health` only | no | no | no | no |
| Operator key | no: `/operator/v1` only | no | no | no | no |

A request without a valid credential answers `401`; a valid credential without the permission answers `403`.

## Common rules

- Every response carries `Cache-Control: no-store` and a strict `Content-Security-Policy`, and no CORS header: no route is meant to be called from another site's page.
- A write from a browser must come from the board's own origin, or from `--public-origin` behind a proxy. Anything else answers `403`.
- `X-Tandem-Agent`, where it is read, must be lowercase letters, digits and hyphens, or the request answers `400`.

## GET /health

Answers `200` with the body `ok` (`text/plain`) while the server is running. It needs no credential in any mode, so a load balancer or a service manager can probe it.

```sh
$ curl -s http://127.0.0.1:7700/health
ok
```

It says the process is up, not that the store is healthy. For that, run `tandem doctor` on the server, or read the `health` object in the MCP `initialize` answer.

## POST /mcp

The MCP endpoint: JSON-RPC 2.0 over streamable HTTP, one JSON response per request. `GET /mcp` answers `405`. Headers, the call envelope, the errors and every tool are in the [MCP reference](mcp.md).

## GET /events

A [server-sent events](https://html.spec.whatwg.org/multipage/server-sent-events.html) stream that tells a client when the store changed, from any door: the board, an agent or a CLI run.

```sh
$ curl -sN http://127.0.0.1:7700/events
event: changed
data: 3

event: changed
data: 4
```

- Every event is named `changed`, and its data is the store's **revision**, a number that grows with each write. The stream opens with the current revision, so a client always has a starting point.
- An event says only that something changed, not what. Re-read what you show (for example with `ticket_list`) when the revision moves. A client that falls behind may skip revisions; the next event still carries the current one.
- The stream stays open with no timeout. Behind a proxy, turn off buffering for this route ([reverse proxy](reverse-proxy.md)).
- A stream opened by a browser session closes when that session ends: sign-out, removal, a revoked key or a role change.

## GET /export

Downloads the whole home (the store and `bikeshed/`) as one `.tar.gz`, the same archive as [`tandem-cli export`](cli.md#export-and-import).

```sh
curl -fsS -H "Authorization: Bearer $TANDEM_TOKEN" -o tandem-home.tar.gz https://board.example.com/export
```

- Allowed for the owner token and an admin's browser session, and for anyone on a loopback or open server. A personal token is refused `403`, whatever its member's role.
- From a browser, the request must come from the board itself (the menu's Export workspace). A request another site triggers is refused `403`.
- Each export writes one row to the access record before it starts. An export that cannot be recorded does not start.
- The archive is not encrypted, and carries no sign-in session and no token. Restore it with [`tandem-cli import`](cli.md#export-and-import) into an empty home.

## The operator API

`/operator/v1` lets a hosting service add, change the role of, remove and count members, and nothing else. It is off unless `TANDEM_OPERATOR_KEY` is set. Its routes, bodies and status codes: [Operator API](operator.md).

## Stability

`/operator/v1` is versioned by its path: a change that breaks a shape or a status code becomes `/operator/v2`. The board's own routes are internal to the board and may change in any release. What every other surface promises: [versions and compatibility](versioning.md).
