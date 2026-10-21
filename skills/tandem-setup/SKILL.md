---
name: tandem-setup
description: "Set up Tandem's agent face on this machine — link the skills, the agent prompts and the AGENTS.md into the agent host, and check that tandem-cli is on PATH. Reach for it when the user wants to install or repair Tandem's setup, when they have just cloned the repo on a new machine, or when the skills and the agents are missing from the host."
---

# Tandem setup

One pass that makes the workflow work on this machine: the skills in `skills/`, the agent prompts in `agents/`, and this repo's `AGENTS.md`, all reachable from the agent host, plus the `tandem-cli` every one of them calls.

Nothing is copied. Every file stays in the checkout and the host gets a **symlink**, so an edit in the repo is an edit the host sees — and a checkout on a branch without the files leaves the links dangling, which this pass reports.

## Find the repo

The checkout is the directory that holds `skills/tandem/SKILL.md` and `agents/`. In order:

1. the human named it;
2. the working directory is a clone of it;
3. this skill's own file, resolved — the host links it, so `pwd -P` resolves the link, and the repo is two levels up:

```sh
here="$(cd "$(dirname "<the path you read this file from>")" && pwd -P)"
repo="$(cd "$here/../.." && pwd -P)"
```

Check it before doing anything:

```sh
test -f "$repo/skills/tandem/SKILL.md" && test -d "$repo/agents" && echo "repo: $repo" || echo "not the tandem checkout: $repo"
```

## The host's directories

| host | skills | agent prompts | instructions |
| --- | --- | --- | --- |
| OpenCode | `${XDG_CONFIG_HOME:-$HOME/.config}/opencode/skills/` | `…/opencode/agents/` | `…/opencode/AGENTS.md` |
| Claude Code | `~/.claude/skills/` | — | `~/.claude/CLAUDE.md` |

OpenCode discovers all three by convention: it scans `skills/` and `agents/` under its config directory and loads `AGENTS.md` there as the global instructions file. Nothing has to be listed in `opencode.json`.

For another host, link the skills where that host reads them, and ask the human where its agent prompts go.

## Link

The three links, for OpenCode. Each one is idempotent and none of them overwrites a real file.

```sh
host="${XDG_CONFIG_HOME:-$HOME/.config}/opencode"
mkdir -p "$host/skills" "$host/agents"

link() { # link <source> <target> <label>
  if [ -L "$2" ]; then
    if [ ! -e "$2" ]; then
      ln -sfn "$1" "$2"; echo "replaced $3 (its target was gone)"
    elif [ "$(readlink "$2")" = "$1" ]; then
      echo "ok       $3"
    else
      echo "skip     $3 (points elsewhere: $(readlink "$2"))"
    fi
  elif [ -e "$2" ]; then
    echo "skip     $3 (a real file is there — compare it, move it aside, then re-run)"
  else
    ln -s "$1" "$2"; echo "linked   $3"
  fi
}

for dir in "$repo"/skills/*/; do
  [ -f "$dir/SKILL.md" ] || continue
  name="$(basename "$dir")"
  link "$repo/skills/$name" "$host/skills/$name" "skills/$name"
done

for file in "$repo"/agents/*.md; do
  [ -e "$file" ] || continue
  name="$(basename "$file")"
  link "$repo/agents/$name" "$host/agents/$name" "agents/$name"
done

link "$repo/AGENTS.md" "$host/AGENTS.md" "AGENTS.md"
```

A link whose target has gone is replaced: it cannot work, and the file it should point at is right here. A link that points somewhere else is left alone and reported — a human may have pointed it deliberately. A real file is never overwritten.

## The CLI

The prompts call `tandem-cli` by name, so it has to be on PATH. The app bundles it.

```sh
command -v tandem-cli || echo "missing — install the app: brew install --cask gianniacquisto/tap/tandem"
```

The cask links the bundled binary into the Homebrew bin. On a machine where the app is installed but the link is not:

```sh
ln -s "/Applications/Tandem.app/Contents/MacOS/tandem-cli" "$(brew --prefix)/bin/tandem-cli"
```

## The duplicate routes

If the host's config still lists this checkout — `AGENTS.md` under `instructions`, or the checkout's `skills/` under `skills` — those entries are a second route to the same files, and the host loads them twice. The links above are the route; the entries are the leftover.

```sh
grep -n "instructions\|skills" "${XDG_CONFIG_HOME:-$HOME/.config}/opencode/opencode.json" 2>/dev/null
```

Remove the entries that point at this checkout, and say which you removed. Leave every other entry alone.

## Verify

```sh
opencode api agent.list   # reviewer, retro, consolidator, refine
opencode api skill.list   # grilling, implement-spec, retro, tandem, tandem-board, tandem-consolidate, tandem-handoff, tandem-memory, tandem-setup, to-spec, writing-for-agents
tandem-cli doctor         # the skills check reads "opencode configured"
```

A host that has not picked the change up yet needs a new session, not a restart of anything.

## Report

Say what was linked, what was skipped and why, what was removed from the host config, and what is left for the human — a CLI that is not on PATH, a real file standing where a link belongs, a link that points elsewhere. A setup that reports a skip is done; a setup that reports nothing is not.

## Done when

- The host lists every agent prompt and every skill this repo ships.
- `tandem-cli doctor` reports the skills check as configured.
- Nothing in the host config points at a path that does not exist, and no file was overwritten.
