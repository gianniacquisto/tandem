# Self-hosting Tandem

This page takes you from a fresh install to a team board: the first admin signed in, more people invited and their AI coding agents connected. Follow it top to bottom. Each step says what you should see before you go on.

## Pick a shape

| You want | Run | Sign-in | Go to |
| --- | --- | --- | --- |
| A board for yourself on your own machine | `tandem serve` | None: loopback is trusted | The [README](../README.md#run). Nothing on this page applies. |
| A board several people share | `tandem serve` on a server, behind HTTPS | Each person signs in as themselves | This page, from [Before you start](#before-you-start). |
| To try team mode on your laptop first | `tandem serve` on `0.0.0.0`, opened at `localhost` | As for a team | [Try it locally](#try-it-locally) |

A team board needs three things a solo board does not: an owner token, a non-loopback address, and HTTPS in front.

## The credentials

Four credentials are involved. Each one opens one thing, and none of them stands in for another.

| Credential | Looks like | Who holds it | What it opens |
| --- | --- | --- | --- |
| **Owner token** | `TANDEM_TOKEN`, 32 or more characters you choose | Whoever runs the server | Everything: the login page, every screen, `/mcp`. It is the bootstrap and recovery credential. |
| **Sign-in key** | `tdk_...` | One person | A browser session as that person, through the login page. Never a bearer. |
| **Personal token** | `tdm_...` | One person, pasted into their agent's MCP settings | `/mcp` as that person, and nothing else. |
| **Operator key** | `TANDEM_OPERATOR_KEY`, optional | A hosting service or script | `/operator/v1` only: add, re-role, remove and count people. It never issues a key. See [operator.md](operator.md). |

The owner token and the operator key are environment variables, never flags (a flag shows in a process listing). Sign-in keys and personal tokens are made by the server and shown once.

## Before you start

You need:

- `tandem` installed on the server ([install paths](install-paths.md)).
- A public name for the board, such as `https://board.example.com`, and a TLS reverse proxy (Caddy, nginx, a load balancer) that will forward to Tandem. TLS ends at the proxy: `tandem serve` speaks plain HTTP.
- An owner token. Make one and keep it in your password manager:

  ```sh
  openssl rand -hex 32
  ```

## 1. Start the server

```sh
export TANDEM_TOKEN='<the owner token>'
tandem serve --host 0.0.0.0 --port 7700 --public-origin https://board.example.com
```

- `--host 0.0.0.0` is what turns token mode on. On loopback (`127.0.0.1`, the default) `TANDEM_TOKEN` is ignored and there is no login page.
- Off loopback the server refuses to start (exit 2) unless `TANDEM_TOKEN` is set to 32 or more characters.
- `--public-origin` is the exact origin the browser sees: no trailing slash, no path. Without it, browser writes through a proxy that rewrites `Host` are refused.
- The home (the database and notes) is `~/.tandem` for the user running the server. Set `TANDEM_HOME` to put it elsewhere.

**You should see** `tandem serve: listening on http://0.0.0.0:7700`, then the origin on its own line.

Point the proxy at `http://127.0.0.1:7700` (or the server's address). It must forward `Authorization` and stream `/events` unbuffered. The details and a symptom table are in [reverse-proxy.md](reverse-proxy.md).

## 2. Sign in as the owner

Open `https://board.example.com`. You land on the login page. It has one field, **Access token**: paste the owner token there and sign in. The same field takes a person's `tdk_` sign-in key later.

**You should see** the board, empty. You are signed in as the owner, who is not a person on the board: the owner exists to set the team up and to recover it.

## 3. Bootstrap the first admin

The owner adds the first admin. There is no other bootstrap path: no first-visitor signup, no variable that makes an admin on start.

1. Go to `https://board.example.com/people`. Type the address: before the first person exists, the menu does not link the screen.
2. Add a person: their name and the role **admin**. The first person must be an admin, and the server refuses anything else.
3. The next page shows their **sign-in key** (`tdk_...`) once. Copy it now. It is never shown again. A lost key is reissued, not recovered.

**You should see** the person listed on `/people`. The home is now in team mode, and stays in it.

If the admin is you, sign out (the menu's Sign out), then sign back in with the `tdk_` key. From here on, use your own key day to day and keep the owner token for recovery.

## 4. Invite the team

Signed in as an admin, open People from the menu. For each person:

1. Add them with a name and a role: **admin** manages people, **member** works on the board.
2. Copy the sign-in key from the page that shows it.
3. Send it to them over a private channel, with the board's address.

They open the board, paste the key into the **Access token** field of the login page, and are in as themselves. Their browser stays signed in for 30 days since its last use.

On the same screen an admin reissues a key with the **Reissue sign-in key** link on a row (the old one stops at once), changes a role, removes a person (which signs them out and ends their keys and tokens at once), revokes someone's personal token, and reads the access record. The last admin cannot be removed or demoted.

## 5. Connect each person's agent

Each person does this for their own agent, signed in as themselves:

1. Open the menu, choose **Personal tokens**, name the token (for example "Claude Code on my laptop") and create it.
2. Copy the `tdm_` token from the page that shows it. It is shown once.
3. Put it in the agent's MCP settings. For Claude Code (`~/.claude.json`, or a project's `.mcp.json`):

   ```json
   {"mcpServers": {"tandem": {"type": "http", "url": "https://board.example.com/mcp", "headers": {"Authorization": "Bearer tdm_..."}}}}
   ```

The agent then works on the board as that person, and its writes record their name. Leave `X-Tandem-Agent` out: with a personal token it may only name the token's own member, and any other name, such as `claude`, is refused `403`. A personal token opens `/mcp` only: it cannot sign in a browser, export, manage people or make more tokens.

## Run it as a service

Keep the token out of the unit file and out of shell history: put it in a file only the service user can read. A systemd example, with the home in `/var/lib/tandem`:

```ini
# /etc/tandem/env  (chmod 600, owned by the tandem user)
TANDEM_TOKEN=<the owner token>
TANDEM_HOME=/var/lib/tandem
```

```ini
# /etc/systemd/system/tandem.service
[Unit]
Description=Tandem board
After=network.target

[Service]
User=tandem
EnvironmentFile=/etc/tandem/env
ExecStart=/usr/local/bin/tandem serve --host 0.0.0.0 --port 7700 --public-origin https://board.example.com
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Adjust `ExecStart` to where `tandem` is installed. Do not bind `127.0.0.1` here even when the proxy runs on the same machine: token mode needs a non-loopback bind. Use `0.0.0.0` with a firewall that lets only the proxy reach the port, or the private address the proxy connects to.

Backups, moving a home and upgrading: [store.md](store.md).

## Recovery

- **A person lost their sign-in key.** An admin reissues it on `/people`.
- **Every admin is locked out.** Sign in with the owner token. The owner still opens `/people` and can reissue an admin's key or add a new admin.
- **The owner token leaked.** Set a new `TANDEM_TOKEN` and restart. Every browser is signed out, people sign back in with their own keys. Then read the access record on `/people` for acts by `owner` you did not make, and reissue or remove any key made with the old token.

## Try it locally

To try team mode on your own machine with no proxy and no domain:

```sh
export TANDEM_TOKEN="$(openssl rand -hex 32)"
echo "$TANDEM_TOKEN"   # you paste this into the login page
TANDEM_HOME="$(mktemp -d)" tandem serve --host 0.0.0.0 --port 7799
```

Then follow steps 2 to 5 with `http://localhost:7799` in place of `https://board.example.com`, and leave out `--public-origin`.

- **Open it at `http://localhost:7799`.** Not at `http://0.0.0.0:7799` and not at your LAN address. The session cookie is `Secure`, and a browser keeps it only over HTTPS or on `localhost`. Anywhere else the login is accepted, then the login page comes back saying the browser did not keep the sign-in.
- `0.0.0.0` is reachable from your network. Use a throwaway home, and stop the server when you are done.
- The `mktemp` home is temporary: the people you add go with it.

## The operator interface

You do not need it to self-host. `/operator/v1` is for a hosting service, or your own provisioning script, to add and remove people without a browser. It is off unless `TANDEM_OPERATOR_KEY` is set when the server starts.

A person added through it has **no sign-in key yet**: the operator never holds a credential that signs in as someone. To get them in, the owner or an admin opens `/people`, uses the **Reissue sign-in key** link on that person's row and confirms, then hands the key over as in step 4. Routes and rules: [operator.md](operator.md).

## When it goes wrong

| You see | Cause | Fix |
| --- | --- | --- |
| `tandem serve: ... is not a loopback address. Set TANDEM_TOKEN ...`, exit 2 | No owner token on a non-loopback bind | Set `TANDEM_TOKEN` (32 or more characters) in the server's environment. |
| `TANDEM_TOKEN must be at least 32 characters`, exit 2 | The token is too short | Make one with `openssl rand -hex 32`. |
| No login page, and `/people` answers `404` | The server is on loopback, so token mode is off | Start it with `--host 0.0.0.0` (or another non-loopback address). |
| The login page says "this browser did not keep the sign-in" | The `Secure` cookie was dropped: the board was opened over plain HTTP on an address that is not `localhost` | Open it over HTTPS, or at `http://localhost:<port>` when trying it locally. |
| Browser writes answer `403` "the request came from another origin" | `--public-origin` does not match the address bar, or is unset behind a proxy | See [reverse-proxy.md](reverse-proxy.md#symptoms). |
| `curl: (35) Send failure: Broken pipe` | `https://` sent straight to `tandem serve`, which speaks plain HTTP | Use `http://` against the serve, or go through the proxy. |
| An agent gets `403` on `/mcp` with a personal token | `X-Tandem-Agent` names someone other than the token's member | Drop the header. |
| `/operator/v1` answers `404` | `TANDEM_OPERATOR_KEY` was not set when the server started | Set it and restart, or add people on `/people` instead. |
| A person added through the operator cannot sign in | The operator issues no key | Reissue their key on `/people`. |

The rules behind this page: [ADR 0049](adr/0049-a-browser-signs-in-to-a-token-guarded-serve.md) (the browser login) and [ADR 0052](adr/0052-members-and-personal-tokens.md) (people, keys and tokens).
