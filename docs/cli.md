# CLI reference

The installer puts the binary in `~/.local/bin` and links `tandem-cli` to it, so both names are the same executable. Add `~/.local/bin` to PATH if the installer says it is missing:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Every command prints JSON on stdout.

```sh
tandem-cli ticket list [--filter open|done|all] [--status <value[,value]>]
                       [--assignee <name|self>] [--lane <name>] [--text <string>]
                       [--archived] [--blocked]
tandem-cli ticket show --id <id>
tandem-cli ticket add --title <string> [--status <value>] [--assignee <name|self>]
                      [--lane <name>] [--origin <name|self>] [--description <string|->]
tandem-cli ticket set --id <id> [--title <string>] [--status <value>]
                      [--assignee <name|self>] [--description <string|->]
                      [--append-description <string|->]
tandem-cli ticket move --id <id> [--lane <name>] [--index <number>]
tandem-cli ticket block --id <id> --on <id>
tandem-cli ticket unblock --id <id> [--on <id>]
tandem-cli ticket complete --id <id>
tandem-cli ticket archive [--id <id>]
tandem-cli ticket comment add --id <id> [--author <name>] --body <string|->
tandem-cli ticket comment list --id <id>

tandem-cli lane list
tandem-cli lane add --name <name> [--abbr <key>] [--project <name>]
tandem-cli lane rename --from <name> --to <name> [--abbr <key>] [--project <name>]
tandem-cli lane delete --name <name>
tandem-cli lane move --name <name> [--index <number>]
tandem-cli project list
tandem-cli project add --name <name>
tandem-cli project rename --from <name> --to <name>
tandem-cli project delete --name <name>

tandem-cli note rename --from <path> --to <path>
tandem-cli note stats [--project <name>]
tandem-cli note links
tandem-cli doctor
```

The Statuses are `backlog`, `on-hold`, `ready`, `in-progress`, `in-review` and `done` (`on-hold` is the parked slot: a blocker, a human decision pending); a ticket's cycle lane is named with `--lane`, and the unnamed default area always comes first. A ticket in a lane carries a **ref** - the lane's key and a per-lane number, `TAN-42` - and every `--id` (or `--ticket`) accepts it beside the full UUID; the default area's tickets have none, and a lane's key is frozen once its tickets carry numbers.

`todo` still answers for `ticket` as a deprecated alias: same parse, same output, one line on stderr saying so, and the `list` reply carries both `tickets` and `todos` with the same array ([ADR 0030](adr/0030-one-entity-the-ticket.md)). The alias and the doubled reply go in the release after the one that deprecated them.

That block is `main`'s grammar, the trunk the next release is cut from: `tandem-cli --help` prints the whole of it, and `--help` on any subcommand or verb prints that subcommand's own usage (`tandem-cli note rename --help` the `note rename` line) on stdout, exit 0.

## Agent examples

The app no longer carries an agent face (ADR 0045). The skills and the pass prompts live in `examples/` for this repository's own workflow: a clone is the copy, and a host reads them from there or from a checkout. There is no `tandem-cli setup` verb, no `<TandemHome>/agent/` installed copy and no host symlink to keep in step.
