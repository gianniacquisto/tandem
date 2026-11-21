# Configuration reference

Tandem has no configuration file. A run is configured by environment variables and command-line flags, and everything it keeps lives in one folder, the **home**. This page lists every setting in one place. The guides explain when you need each one: [self-hosting](self-hosting.md), [reverse proxy](reverse-proxy.md), [install paths](install-paths.md).

## The home

The home holds the store (`tandem.db`), its rolling backup and the notes (`bikeshed/`). Every command resolves it the same way, first match wins:

1. The `--home <path>` flag, on any verb.
2. The `TANDEM_HOME` environment variable, when it is set and not empty.
3. `~/.tandem` (`%USERPROFILE%\.tandem` on Windows).

A leading `~` in either override is expanded to your home directory, and a relative path is taken from the working directory. `tandem doctor` prints the home it resolved and which of the three named it. What each file in the home is, and which are safe to delete: [the store](store.md#what-lives-in-the-home).

## Environment variables

### Everyday

| Variable | Read by | Default | Meaning |
| --- | --- | --- | --- |
| `TANDEM_HOME` | every command | `~/.tandem` | The home. See [above](#the-home). |
| `TANDEM_AGENT` | `tandem-cli`, `tandem serve` | unset: `human` | The name a write is recorded under. Locally it is the CLI run's actor. Through `TANDEM_SERVER` it is sent as the `X-Tandem-Agent` header. On `tandem serve` it is the server's own actor, used for an `/mcp` call that names none. Blank or whitespace counts as unset. |
| `TANDEM_SERVER` | `tandem-cli` | unset: the local store | The URL of a running `tandem serve`, such as `http://127.0.0.1:7700`. Every verb then runs as one MCP call to `<url>/mcp` instead of opening the store. `serve`, `export`, `import`, `note rename` and `note stats` refuse by name. See [the remote target](cli.md#remote-target). |
| `TANDEM_LOG` | `tandem serve` | `info` | The log filter, in [`RUST_LOG` directive syntax](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives), such as `debug` or `warn,tandem_core=debug`. An invalid value falls back to the default, and the server says so once. The log goes to stdout. |

### Serving to a network

These matter only when `tandem serve` binds a non-loopback `--host`. On loopback (the default, `127.0.0.1`) the server trusts the machine, ignores `TANDEM_TOKEN` and shows no login page.

| Variable | Read by | Default | Meaning |
| --- | --- | --- | --- |
| `TANDEM_TOKEN` | `tandem serve`, `tandem-cli` | unset | **Serve:** the owner credential, at least 32 characters. Off loopback, every route but `/health` then wants `Authorization: Bearer <token>`, and a browser signs in once at `/login`. **CLI:** with `TANDEM_SERVER`, sent as the bearer header. The CLI refuses to send it over plain `http://` to a host that is not loopback, and never prints it. |
| `TANDEM_PUBLIC_ORIGIN` | `tandem serve` | unset | The one origin a browser sees behind a TLS proxy, such as `https://board.example.com`. Same rules as [`--public-origin`](#tandem-serve); the flag wins when both are set. |
| `TANDEM_OPERATOR_KEY` | `tandem serve` | unset: `/operator/v1` answers `404` | Opens the [operator API](operator.md) and nothing else. At least 32 characters, and never equal to `TANDEM_TOKEN`. |
| `TANDEM_OPERATOR_KEY_NEXT` | `tandem serve` | unset | A second operator key accepted beside the first, for [rotation without downtime](operator.md#the-key). Only valid when `TANDEM_OPERATOR_KEY` is set. |

Secrets are environment variables, never flags, because a flag shows in a process listing. An empty variable counts as unset everywhere on this page.

### The installer

`install.sh` reads two variables. Its flags do the same and win when both are given.

| Variable | Flag | Default | Meaning |
| --- | --- | --- | --- |
| `TANDEM_INSTALL_DIR` | `--bin-dir <dir>` | `~/.local/bin` | Where `tandem` and the `tandem-cli` link go. Use the same value on every run. |
| `TANDEM_VERSION` | `--version <x.y.z>` | the newest release | The release to install. |

The installer's modes and exit codes are in [install paths](install-paths.md#install-upgrade-reinstall-uninstall).

## Flags

### Every verb

| Flag | Meaning |
| --- | --- |
| `--home <path>` | Use this home for this run. Wins over `TANDEM_HOME`. Ignored under `TANDEM_SERVER`, where the server's home is the one used. |
| `--help`, `-h` | Print the usage of the verb it follows, on stdout, and exit `0`. |
| `--version` | Print the release, such as `tandem 2.7.0`. |

### `tandem serve`

| Flag | Default | Meaning |
| --- | --- | --- |
| `--host <ip>` | `127.0.0.1` | The address to bind. Must be an IP address, not a host name. A non-loopback address needs `TANDEM_TOKEN` or `--allow-unauthenticated`. |
| `--port <port>` | `7700` | The port to bind. `0` asks the OS for a free one, and the server prints the port it got. |
| `--open` | off | Open the board in the system browser once the server is listening. |
| `--public-origin <origin>` | `TANDEM_PUBLIC_ORIGIN` | The origin a browser sends behind a TLS proxy: `https://host[:port]`, lowercase host, no path, no trailing slash, no userinfo. `:443` is dropped. Browser form writes and `/mcp` then accept that origin and no other. See [reverse proxy](reverse-proxy.md). |
| `--allow-unauthenticated` | off | Bind a non-loopback address with no authentication at all, for a network you trust. Never use it on a public name. |

The server stops on `SIGINT` (Ctrl-C) or `SIGTERM`. A changed setting takes effect at the next start.

Each page the server draws carries its release as `data-version` on `<html>`, so a server left running across an upgrade shows in the browser. A request slower than 500 ms (on any route but `/events` and `/health`) logs a `slow request` warning with its route and latency.

### Start-up refusals

`tandem serve` checks its settings before it binds and exits `2`, with the reason on stderr, when:

- `--host` is not an IP address.
- `--host` is not loopback and neither `TANDEM_TOKEN` nor `--allow-unauthenticated` is given.
- `TANDEM_TOKEN` is set, off loopback, with fewer than 32 characters.
- `--public-origin` or `TANDEM_PUBLIC_ORIGIN` is malformed, or is set while `--host` is loopback.
- `TANDEM_OPERATOR_KEY` or `TANDEM_OPERATOR_KEY_NEXT` has fewer than 32 characters, equals `TANDEM_TOKEN`, or `_NEXT` is set without the first.
- `--allow-unauthenticated` is given and the home holds members ([team mode](self-hosting.md)), which it would ignore.

No message ever prints a secret's value.

## Connecting an agent

An agent host needs the endpoint and, optionally, two headers. Ready-made settings for Claude Code and opencode are in the [README](../README.md#connect-your-agent).

| Setting | Value |
| --- | --- |
| Transport | MCP, streamable HTTP |
| URL | `http://127.0.0.1:7700/mcp`, or `<public origin>/mcp` for a hosted board |
| `X-Tandem-Agent` header | The name the agent's writes are recorded under: lowercase letters, digits and hyphens. Without it, writes record the server's `TANDEM_AGENT`, else `human`. On a member's personal token the header may only be absent or name that member. |
| `Authorization` header | `Bearer <token>`, needed off loopback: the owner token, or a member's personal token (`tdm_...`) in [team mode](self-hosting.md). |

## Exit codes

Every `tandem` and `tandem-cli` verb uses the same three codes. Results go to stdout as JSON, and errors go to stderr.

| Code | Meaning |
| --- | --- |
| `0` | Success. |
| `1` | The request was understood and refused or failed: an unknown ticket, a rule the store enforces, a failed read or write, a remote server that refused the token. |
| `2` | A usage error (an unknown verb or flag, a missing value), or a setting `tandem serve` refuses at start-up. The usage text is printed to stderr. |

`tandem doctor` exits `1` when any check fails, and still prints the whole report: each check's `ok` and `detail` say which one failed.
