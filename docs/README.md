# Tandem documentation

Tandem is one binary, `tandem serve`, that gives you and your AI coding agents one shared ticket board and one folder of markdown notes. You reach it in the browser, your agents reach it over MCP at `POST /mcp`, and scripts reach it through `tandem-cli`. Everything lives in one local folder, `~/.tandem`, with no account.

## Get started

- [Install](../README.md#install): the one-line installer for macOS and Linux, and the Windows zip.
- [Run](../README.md#run) the board and [connect your agent](../README.md#connect-your-agent).
- [First run](../README.md#first-run): file a ticket from your agent and watch it land on the board.

## Understand

- [Concepts](concepts.md): tickets, refs, projects, labels, statuses, blocks, notes and the home.

## Guides

| Guide | Read it when you want to |
| --- | --- |
| [Installing, upgrading and uninstalling](install-paths.md) | Pin or roll back a version, uninstall, verify a download's checksum and provenance |
| [Self-hosting](self-hosting.md) | Run one board for a team, each person signing in as themselves with their own agents |
| [Serving behind a reverse proxy](reverse-proxy.md) | Put `tandem serve` behind HTTPS on a public name |
| [The store, backup and notes](store.md) | Know what is in `~/.tandem`, restore the backup, move a home to another machine, upgrade the schema |
| [Agent examples](../examples/README.md) | Copy agent instructions, skills and pass prompts that work with the board |

## Reference

| Reference | Covers |
| --- | --- |
| [Configuration](configuration.md) | Every environment variable, every `tandem serve` flag, defaults, precedence and exit codes |
| [CLI](cli.md) | Every `tandem-cli` verb and its flags, export and import, the remote target |
| [MCP tools](mcp.md) | Every tool an agent gets at `POST /mcp`, generated from the server's own registry |
| [Operator API](operator.md) | `/operator/v1`, the HTTP interface a hosting service uses to manage members |

## Background

- [Architecture](architecture.md): the pieces and how they talk.
- [Architecture decision records](adr/README.md), indexed by which are still in force.
- [Glossary](../GLOSSARY.md): the project's vocabulary.

## Contributing

- [Contributing](../CONTRIBUTING.md), including running from source.
- [Releasing](releasing.md): how a tag becomes a release.
- [Design](../DESIGN.md): the board's visual style.
