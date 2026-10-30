---
name: tandem-setup
description: "Set up Tandem's agent face on this machine: install the bundled skills, agent prompts and AGENTS.md into the agent host with `tandem-cli setup install`, and report what it did. Reach for it when the user wants to install or repair Tandem's setup, when they have just installed the app on a new machine, or when the skills and the agents are missing from the host."
---

# Tandem setup

The artifact carries the agent face: every skill, the four prompts and `AGENTS.md`. `tandem-cli setup` is its only writer (ADR 0034). Install copies the face once to `<TandemHome>/agent/` and points the host's paths at that copy. Nothing is copied or linked by hand.

## Install

Read the state first, then install:

```sh
tandem-cli setup status
tandem-cli setup install
```

A bare `setup install` writes OpenCode: `~/.config/opencode/skills/<name>` for every skill, `~/.config/opencode/agents/<name>.md` for every prompt, and one `instructions` entry in `~/.config/opencode/opencode.json` naming `<TandemHome>/agent/AGENTS.md`. `--host claude` or `--host agents` names another host, and a host is written only when it is named.

Report the JSON the run prints: each host's `state` (`installed`, `checkout`, `real-file` or `absent`), the `installedVersion` beside `bundledVersion`, and every warning with its `hint`. That report is the answer.

- `blocked-path`: a real file sits where a link would go. Tell the human the path; moving it is theirs, and install never replaces it, `--force` included.
- `double-load`: `~/.config/opencode/AGENTS.md` still exists beside the `instructions` entry. Removing it is the human's call.
- `unwritable`: nothing was written. The hint names the fix.

## The checkout route

On a machine working on Tandem itself, the host's links already resolve into the checkout. Install leaves them and reports each as `foreign-link` (`info`): that is the normal state there, and `tandem-cli doctor` stays green. `tandem-cli setup install --force` replaces them with the installed copy, and only when the human asks.

## tandem-cli on PATH

The cask links the bundled binary into Homebrew's `bin`, so `tandem-cli` is on PATH after the install. Where it is not:

```sh
command -v tandem-cli || echo "missing: brew install --cask gianniacquisto/tap/tandem"
ln -s "/Applications/Tandem.app/Contents/MacOS/tandem-cli" "$(brew --prefix)/bin/tandem-cli"   # if the link is missing
```

## Verify

```sh
opencode api agent.list   # the four prompts
opencode api skill.list   # every skill the artifact ships; the host's list is the authority
tandem-cli doctor         # the skills row reads "opencode configured", the agent-face row counts the warnings
```

`opencode api skill.list` is the authority for the set: it lists what the host actually read, so a skill added to the artifact appears there without this file changing. A change to a skill or to `AGENTS.md` is picked up by a new session. An agent prompt is read when the server starts, so it is live only after a host restart: compare `opencode api agent.list` with `<TandemHome>/agent/agents/` to tell which is live.

## Done when

- The host lists every agent prompt and every skill the artifact ships, and `tandem-cli doctor` reads "opencode configured".
- Nothing in the host config points at a path that does not exist, and no file was overwritten.
- The report says what was linked, what was skipped and why, and what is left for the human: a skip is a done setup; silence is not.
