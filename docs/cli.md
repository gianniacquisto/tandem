# CLI reference

The installer puts the binary in `~/.local/bin` and links `tandem-cli` to it, so both names are the same executable. Add `~/.local/bin` to PATH if the installer says it is missing:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Every command prints JSON on stdout.

```sh
tandem-cli ticket list [--filter open|done|all] [--status <value[,value]>]
                       [--assignee <name|self>] [--label <name>] [--text <string>]
                       [--archived] [--blocked]
tandem-cli ticket show --id <id>
tandem-cli ticket add --title <string> [--status <value>] [--assignee <name|self>]
                      [--label <name>] [--origin <name|self>] [--description <string|->]
tandem-cli ticket set --id <id> [--title <string>] [--status <value>]
                      [--assignee <name|self>] [--description <string|->]
                      [--append-description <string|->]
tandem-cli ticket move --id <id> [--label <name>] [--index <number>]
tandem-cli ticket block --id <id> --on <id>
tandem-cli ticket unblock --id <id> [--on <id>]
tandem-cli ticket complete --id <id>
tandem-cli ticket archive [--id <id>]
tandem-cli ticket comment add --id <id> [--author <name>] --body <string|->
tandem-cli ticket comment list --id <id>

tandem-cli label list
tandem-cli label add --name <name> [--abbr <key>] [--project <name>]
tandem-cli label rename --from <name> --to <name> [--abbr <key>] [--project <name>]
tandem-cli label delete --name <name>
tandem-cli label move --name <name> [--index <number>]
tandem-cli project list
tandem-cli project add --name <name>
tandem-cli project rename --from <name> --to <name>
tandem-cli project delete --name <name>

tandem-cli note rename --from <path> --to <path>
tandem-cli note stats [--project <name>]
tandem-cli note links
tandem-cli doctor
```

The Statuses are `backlog`, `on-hold`, `ready`, `in-progress`, `in-review` and `done` (`on-hold` is the parked slot: a blocker, a human decision pending); a ticket's label is named with `--label`, and the unnamed default area always comes first. A ticket in a label carries a **ref** - the label's key and a per-label number, `TAN-42` - and every `--id` (or `--ticket`) accepts it beside the full UUID; the default area's tickets have none, and a label's key is frozen once its tickets carry numbers.

`todo` still answers for `ticket` as a deprecated alias: same parse, same output, one line on stderr saying so, and the `list` reply carries both `tickets` and `todos` with the same array ([ADR 0030](adr/0030-one-entity-the-ticket.md)). The alias and the doubled reply go in the release after the one that deprecated them.

That block is `main`'s grammar, the trunk the next release is cut from: `tandem-cli --help` prints the whole of it, and `--help` on any subcommand or verb prints that subcommand's own usage (`tandem-cli note rename --help` the `note rename` line) on stdout, exit 0.

## Export and import

```sh
tandem-cli export --out <path|->
tandem-cli import <path|-> [--max-bytes <n>]
```

Like `serve`, these two are the human's and the operator's verbs, not an agent's: they are not in the agent grammar above, and `tandem-cli export --help` prints each one's usage.

`tandem-cli export --out <path>` writes the home (the store and `bikeshed/`) as one `.tar.gz`, created private to you (0600); an existing path is refused, never overwritten, and `--out -` streams to stdout. It prints the path and the size, and one record line (who, when, `door=cli`) to stderr. It opens the home directly and needs no `tandem serve`, but runs happily beside one. `tandem-cli import <path>` (`-` reads stdin) restores an archive into an empty home: a home that already has a store, a backup, a journal or notes is refused, with no merge and no force. `--max-bytes <n>` is an optional ceiling on the unpacked total. Neither verb is an MCP tool and both refuse by name under `TANDEM_SERVER`; unset `TANDEM_SERVER` or run them on the machine that holds the home (for import, pass `--home <empty dir>`). The archive carries no sign-in session and no token, and is not encrypted. The board offers the same archive as a download, `GET /export` behind the header menu's Export workspace; a remote user fetches it with `curl` and the bearer header, since no CLI verb or MCP tool reaches it. See [Moving and restoring a home](store.md#moving-and-restoring-a-home) and [ADR 0050](adr/0050-the-home-leaves-and-arrives-as-one-archive.md).

## Agent examples

The app no longer carries an agent face (ADR 0045). The skills and the pass prompts live in `examples/` (start at [examples/README.md](../examples/README.md)) for this repository's own workflow: a clone is the copy, and a host reads them from there or from a checkout. There is no `tandem-cli setup` verb, no `<TandemHome>/agent/` installed copy and no host symlink to keep in step.

## Remote target

Set `TANDEM_SERVER` to a running `tandem serve` (for example `http://127.0.0.1:7700`) and every verb runs against it instead of the local store: each is one MCP `tools/call` to `<url>/mcp`, and prints the same stdout JSON and exit codes as local mode. `TANDEM_TOKEN` (never a flag) is sent as `Authorization: Bearer` when the server was started with one; the CLI refuses to send it to an `http://` URL whose host is not loopback and never echoes it. `TANDEM_AGENT` becomes the `X-Tandem-Agent` header, so the write records the same actor it would locally. `serve`, `export`, `import`, `note rename`, `note stats` and `ticket set --append-description` have no remote form and refuse by name; `--home` is ignored. See [ADR 0047](adr/0047-serve-is-the-single-door.md). Serving behind a TLS proxy on a public name: [reverse-proxy.md](reverse-proxy.md).

A served page carries the release that drew it as `data-version` on `<html>`, so a long-lived serve left running across an upgrade is visible in the browser. A request slower than 500 ms (any route but `/events` and `/health`, `/mcp` included) logs a `slow request` warning with its route and latency after its request line.

In a browser, a token-mode `tandem serve` shows a login page: paste `TANDEM_TOKEN` once and the browser stays signed in for 30 days since last use (a `Secure` session cookie, so reach the server over HTTPS or on localhost). The Sign out button ends the session. `/mcp` and the remote CLI keep the bearer header. See [ADR 0049](adr/0049-a-browser-signs-in-to-a-token-guarded-serve.md).
