# Harness bindings — how each host runs this protocol

The protocol in this directory is **host-neutral**: it names the roles, the invariants, and the cadence, so any host that reads `AGENTS.md` can follow it, and it names no host's agent, command, or config file.

This file records the other half — how each host implements that protocol: the agent that runs the consolidation pass, what triggers it, how the host is registered, whether its instructions reach child sessions, and the flags its tools need for the hidden layer. Add a section per host.

## OpenCode

**Consolidation agent** — `~/.config/opencode/agents/consolidator.md`, `mode: subagent`. Its permissions hold the pass's invariants: `edit` denied on `*.raw/*` (captures are create-only; consuming one is a `mv`), plus `subagent` and `question` denied (the pass is a non-interactive batch that never dispatches and never asks).

**Trigger** — `~/.config/opencode/commands/consolidate.md`, with `agent: consolidator` and `subagent: true`, so `/consolidate` runs the pass in a background child session. Both the agent and the command file **hot-reload**: an edit takes effect without a restart.

**Registration** — `~/.config/opencode/opencode.json` lists this repo's `AGENTS.md` under `instructions` and its `skills/` directory under `skills`.

**Child sessions** — the session's instructions reach child sessions as well as the root's, so a working sub-agent needs no restating in its brief. Verified: an `explore` child reported this repo's `## Memory` section.

**Hidden layer** — the saddlebag is a hidden `.raw/` subtree, so any read of it needs the hidden flags: glob with `hidden: true`, `rg --hidden`, `ls -a`.
