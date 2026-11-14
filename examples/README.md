# Examples

Tandem ships the mechanism: the board, the notes store, the MCP server and the CLI. This folder is the policy: agent instructions, skills and pass prompts you copy, edit or ignore. Nothing here is installed by Tandem, and nothing here is required (ADR 0045).

## Tiers

Each tier works without the next.

| Tier | Copy | You get |
| --- | --- | --- |
| 1. Memory | `AGENTS.md`, `skills/tandem-memory/` | Orient from the board and notes, leave a handoff comment. The smallest set: about 4 KB of instructions. |
| 2. Workflow | `skills/tandem-refine`, `skills/tandem-build`, `skills/tandem-sprint`, `agents/builder.md`, `agents/refiner.md` | Refine, build and run a batch of tickets. |
| 3. Review | `agents/reviewer.md`, `skills/review-lenses/`, `agents/{architect,security,ux}.md` | A merge gate read through named lenses. |

Also here: `skills/tandem-team` (a roster template: replace it with your own), `skills/tandem-tidy`, `skills/tandem-interview`, `skills/retro`, and general skills (`grilling`, `tdd`, `writing-for-agents`).

## Set up

1. **Point your agent host at the MCP server**, once:

   ```json
   {"mcp": {"tandem": {"type": "local", "command": ["tandem-cli", "mcp"]}}}
   ```

   `TANDEM_HOME` sets the home (default `~/.tandem`). `TANDEM_AGENT` names the actor a write records; `self` expands to it.
2. **Copy what you want.** Put `AGENTS.md` where your host reads global instructions and the skill folders where it reads skills, or symlink them to this checkout to track it. Agents use the MCP tools only; `tandem-cli` is for you and your scripts.
3. **Check it.** `tandem-cli doctor` prints `{ok, checks}` with the checks `home`, `store` and `bikeshed`, and exits `0` when healthy.

## Where things live

- **Store**: `~/.tandem/tandem.db` (SQLite) holds tickets, dependencies, comments, lanes and projects. Notes live in `~/.tandem/bikeshed/`.
- **Recovery**: every changed write keeps one previous copy in `tandem.db.bak`. To restore it, journal files first:

  ```sh
  rm -f ~/.tandem/tandem.db-wal ~/.tandem/tandem.db-shm ~/.tandem/tandem.db
  mv ~/.tandem/tandem.db.bak ~/.tandem/tandem.db
  ```

- **Board**: `tandem serve` draws it on `127.0.0.1:7700`. A non-loopback `--host` needs `--allow-unauthenticated`.
- **CLI**: every verb prints one JSON object; exit `0` success, `1` a domain failure, `2` a usage error. `tandem-cli --help` lists the verbs, and [docs/cli.md](../docs/cli.md) documents them.

## Keeping the examples honest

A test (`examples_tools`) fails when an example names an MCP tool that the server does not expose, so a retired verb cannot linger in a prompt.
