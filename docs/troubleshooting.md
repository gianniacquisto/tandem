# Troubleshooting

Start with `tandem doctor`. It checks the install, the home, the store and the notes, and each failing check's `detail` says what to do. It exits `1` when a check fails.

```sh
tandem doctor
```

Then find the symptom below. Each entry gives the likely cause and the fix, and links to the page with the details.

## Installing and upgrading

| You see | Cause | Fix |
| --- | --- | --- |
| `tandem: command not found` | `~/.local/bin` is not on PATH | Add `export PATH="$HOME/.local/bin:$PATH"` to your shell profile, and open a new shell. |
| The installer says `already exists; leaving it alone`, and nothing changed | Running the installer again is not an upgrade | Add `--reinstall`. See [installing](install-paths.md#install-upgrade-reinstall-uninstall). |
| `tandem --version` still shows the old release after an upgrade | A second copy comes first on PATH | `which -a tandem` lists every copy. Remove the old one, or install with the same `--bin-dir` as before. |
| The board or an agent still behaves like the old release | `tandem serve` was started before the upgrade | Stop it and start it again. A served page's `data-version` on `<html>` names the release that drew it. |
| macOS refuses to open a binary unpacked by hand | The download carries the quarantine flag | `xattr -dr com.apple.quarantine <path>`. The installer does this for you. |
| Windows will not replace `tandem.exe` | The binary is running | Stop `tandem serve`, then replace it. |

## Starting the server

| You see | Cause | Fix |
| --- | --- | --- |
| `tandem serve: could not bind 127.0.0.1:7700: Address already in use` | Another `tandem serve`, or another program, holds the port | Use the server already running, stop it, or pick another port with `--port`. |
| `tandem serve: ...`, exit `2`, before it listens | A setting it refuses: a non-loopback `--host` with no token, a short token, a malformed public origin | The message names the rule. Every refusal is listed in [configuration](configuration.md#start-up-refusals). |
| The board reports that the store is a schema this build does not read | An older `tandem` is running against a store a newer one has upgraded | Upgrade every copy and restart the server. See [upgrading](store.md#upgrading). |

## Connecting an agent

| You see | Cause | Fix |
| --- | --- | --- |
| The agent has no Tandem tools | `tandem serve` is not running, or the agent host has the wrong URL | Start `tandem serve` and leave it running, check the port in the agent's MCP settings, and start a new agent session. The Claude Code plugin warns at session start when the server is down. |
| The agent's writes are recorded as `human` | No `X-Tandem-Agent` header | Add the header to the agent's MCP settings. See [connect your agent](../README.md#connect-your-agent). |
| `/mcp` answers `400` | `X-Tandem-Agent` is not lowercase letters, digits and hyphens | Fix the name in the header. |
| `/mcp` answers `401` | Token mode, and the token is missing, wrong or revoked | Send `Authorization: Bearer <token>`. A revoked personal token stays revoked: make a new one. |
| `/mcp` answers `403` with a personal token | `X-Tandem-Agent` names someone other than the token's member | Remove the header: a personal token always writes as its member. |
| `ticket move` removed a ticket's label | Leaving out the label moves the ticket to no label (a known issue) | Name the label with the new position. See [ticket move](cli.md#ticket-move). |

## The remote CLI

| You see | Cause | Fix |
| --- | --- | --- |
| `Cannot reach the tandem server at <url>/mcp: ... Connection refused` | Nothing listens at `TANDEM_SERVER` | Check the URL and that the server is running. |
| `The server refused the token (401). Check TANDEM_TOKEN.` | `TANDEM_TOKEN` is unset, wrong or revoked | Set the right token. |
| `Refusing to send TANDEM_TOKEN over plain http to a non-loopback host.` | The URL is `http://` to another machine | Use the `https://` address behind the proxy. |
| `tandem export: this verb has no remote form` | `export`, `import`, `serve`, `note rename` and `note stats` only work on the machine that holds the home | Unset `TANDEM_SERVER`, or run the verb on the server. |

## A board for a team

| You see | Cause | Fix |
| --- | --- | --- |
| No login page, and `/people` answers `404` | The server is on loopback, so token mode is off | Start it on a non-loopback `--host`. See [self-hosting](self-hosting.md). |
| The login page says the browser did not keep the sign-in | The session cookie is `Secure`, and the board was opened over plain HTTP on an address that is not `localhost` | Use HTTPS, or `http://localhost:<port>` when trying it locally. |
| Browser writes answer `403`, "the request came from another origin" | `--public-origin` is unset behind a proxy, or does not match the address bar | See [reverse proxy](reverse-proxy.md#symptoms). |
| The board does not update live behind a proxy | The proxy buffers `/events` | Turn off buffering for `/events`. See [reverse proxy](reverse-proxy.md). |
| `curl: (35) Send failure: Broken pipe` | `https://` sent straight to `tandem serve`, which speaks plain HTTP | Use `http://` against the server, or go through the proxy. |
| More cases, with people and keys | | [Self-hosting: when it goes wrong](self-hosting.md#when-it-goes-wrong). |

## Data

| You see | Cause | Fix |
| --- | --- | --- |
| A write went wrong a moment ago | | Restore the one-write-old backup. See [the backup](store.md#the-backup). |
| A mistake from several writes ago | The backup holds one generation only | Restore from your own copy or an export. Make one before anything risky with `tandem-cli export`. |
| `tandem doctor` reports orphan comments or other reference failures | Rows that point at a ticket that is gone | Export the home first, then follow the check's `detail`, and [file a bug](#reporting-a-bug) with the ids. |

## Reporting a bug

Open an issue on [GitHub](https://github.com/gianniacquisto/tandem/issues) with `tandem --version`, your platform, the output of `tandem doctor`, and the steps that show the problem. For a security problem, do not open an issue: see the [security policy](../SECURITY.md).
