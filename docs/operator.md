# The operator interface

`/operator/v1` is the narrow door a hosting service (or a self-hoster's own script) uses to add, change the role of, remove and count the members of one instance. It is the only way to manage people with a key that is not a member's. It manages people and counts seats, and nothing else: it never reads a ticket, a note or a comment and never acts as a member. See [ADR 0052](adr/0052-members-and-personal-tokens.md) for members and roles.

The interface is versioned by its path. A change that breaks a shape or a status code is a new prefix, `/operator/v2`.

## The key

The operator key is a credential of its own, held in the serve process's environment:

| Variable | Meaning |
| --- | --- |
| `TANDEM_OPERATOR_KEY` | The key. At least 32 characters, else `tandem serve` exits 2. |
| `TANDEM_OPERATOR_KEY_NEXT` | An optional second key accepted beside it, for rotation. The same rules. Set only beside `TANDEM_OPERATOR_KEY`. |

Never a flag value (a flag shows in a process listing), never stored, never in a file the instance exports. Only a SHA-256 digest is held in memory, compared in constant time. The key is never `TANDEM_TOKEN` (the server refuses to start if they match) and never a member's credential. An empty variable is unset.

**Rotation without downtime.** Set both variables (the old key as `TANDEM_OPERATOR_KEY`, the new one as `TANDEM_OPERATOR_KEY_NEXT`), restart, move the hosting service to the new key, then at the next restart set the new key as `TANDEM_OPERATOR_KEY` and drop `_NEXT`.

With no operator key set the whole family answers `404`.

## Requests

Every request carries `Authorization: Bearer <operator key>` and nothing else that names a caller. The policy is its own, beside the JSON one of `/mcp` and the form one of the board:

- JSON bodies (`Content-Type: application/json`) on `POST` and `PATCH`; another type answers `415`.
- No cookie is read. An `X-Tandem-Agent` header is ignored: the actor is always `operator`.
- An `Origin` header of any value is refused `403`: this is not a browser door. No CORS header is ever answered.
- A missing, malformed or wrong key answers `401` with one body, whoever else the caller is.

## Routes

Every answer is JSON. An error is `{"error": "<short reason>"}` and never carries a secret.

| Route | Body | Answer |
| --- | --- | --- |
| `GET /operator/v1/members/count` | none | `200 {"members": <number>}`. A number only: the human seat count. Agents and personal tokens are never members. Read-only: it writes no row. |
| `POST /operator/v1/members` | `{"name", "role"}`, role `member` or `admin` | `201 {"id", "name", "role"}`. No secret: the operator never receives a credential that opens the board. |
| `PATCH /operator/v1/members/{name}` | `{"role"}` | `200 {"name", "role"}`; `404` for no such member. |
| `DELETE /operator/v1/members/{name}` | none | `200 {"name", "removed": true}`; `404` for no such member. The member's sessions, personal tokens and sign-in key end in the same write. The name is never reused. |

Status codes for refusals: `400` for a name that breaks the rules (1 to 64 lowercase letters, digits or hyphens; `count`, `human`, `operator` and `owner` are reserved; `count` would shadow the `members/count` route), an unknown role, or a body that is not exactly the documented object; `409` for a name that is taken (now, or before a removal), for the last admin (see below) and for a first member who is not an admin; `404` for an unknown member; `401`, `403`, `415` as above.

A new member signs in by an admin issuing a sign-in key (the People screen), or, on a hosted workspace, by a signed assertion. The operator interface issues neither.

## Rules it shares with every door

The four routes call the same store functions as every other door, as the actor `operator`, so there is one writer and one rule:

- **The last admin** cannot be removed or demoted, by the operator either (`409`).
- **The first member must be an admin** (`409`): a workspace is never left with members and no admin. The operator adds the first admin; a member can follow.
- **Every act is recorded.** Each add, role change and removal writes one row of the access record with the actor `operator`, in the same transaction as the change, visible to every admin. A refused or rejected request writes no row, and a count writes none. A rejected request logs a reason and never a value.

## What the key can and cannot reach

It reaches the four routes above and nothing else. On every other route (the board, `/events`, `/export`, `/static`, `/mcp`, `/login`, `/health` and any later one) a request carrying the operator key answers `401`: default deny, so a later route cannot inherit the key. A member's personal token, a member's session, the owner credential (`TANDEM_TOKEN`) and an `X-Tandem-Agent` header never open the family.

The operator may add an admin, because a hosted workspace has no admin until provisioning adds the first. The protection is not that the key is powerless: an operator who can add an admin, or who runs the process, can reach board content through a member it invents. The protection is that the act is recorded in the access record, visible to every admin, and cannot be silent.

```sh
curl -X POST https://workspace.example/operator/v1/members \
  -H "Authorization: Bearer $TANDEM_OPERATOR_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name": "ada", "role": "admin"}'
curl -H "Authorization: Bearer $TANDEM_OPERATOR_KEY" \
  https://workspace.example/operator/v1/members/count
```
