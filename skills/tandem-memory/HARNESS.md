# Harness bindings — how each host runs this protocol

The protocol in this directory is **host-neutral**: it names the roles, the invariants, and the record rule, so any host that reads `AGENTS.md` can follow it, and it names no host's agent, command, or config file.

This file records the other half — how each host implements that protocol: the agent that runs the consolidation pass, what triggers it, how the host is registered, whether its instructions reach child sessions, and the flags its tools need for the hidden layer. Add a section per host.

## OpenCode

**Consolidation agent** — `agents/consolidator.md` in this repo, linked to `~/.config/opencode/agents/consolidator.md` by the `tandem-setup` skill. `mode: subagent`. Its permissions hold the pass's invariants: `edit` denied on `*.raw/*` (captures are create-only; consuming one is a `mv`), plus `subagent` and `question` denied (the pass is a non-interactive batch that never dispatches and never asks).

**Trigger** — the `tandem-consolidate` skill in this repo, linked to `~/.config/opencode/skills/tandem-consolidate/`. It dispatches the `consolidator` subagent with a one-line brief and reports the result. A host may also bind a command to the same agent — the binding is the host's business, the skill is the protocol's.

**Registration** — the `tandem-setup` skill links this repo's `AGENTS.md` to `~/.config/opencode/AGENTS.md` and each of its `skills/*` to `~/.config/opencode/skills/*`. The host discovers all three by convention, so no `opencode.json` entry is needed — and an entry pointing at the same files would load them twice.

**Child sessions** — the session's instructions reach child sessions as well as the root's, so a working sub-agent needs no restating in its brief. Verified: an `explore` child reported this repo's `## Memory` section.

**Hidden layer** — the saddlebag is a hidden `.raw/` subtree, so any read of it needs the hidden flags: glob with `hidden: true`, `rg --hidden`, `ls -a`.
