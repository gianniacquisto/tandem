---
name: tandem-setup
description: "Set up Tandem's agent face on this machine — link the skills, the agent prompts and the AGENTS.md into the agent host, and check that tandem-cli is on PATH. Reach for it when the user wants to install or repair Tandem's setup, when they have just cloned the repo on a new machine, or when the skills and the agents are missing from the host."
---

# Tandem setup

Nothing is copied — the host gets **symlinks** into the checkout, so an edit in the repo is an edit the host sees. A checkout on a branch without the files leaves the links dangling; this pass reports that.

## Find the repo

The checkout holds `skills/tandem/SKILL.md` and `agents/`. In order: the human named it; the working directory; this skill's own file resolved (`pwd -P` — the repo is two levels up):

```sh
here="$(cd "$(dirname "<the path you read this file from>")" && pwd -P)"
repo="$(cd "$here/../.." && pwd -P)"
test -f "$repo/skills/tandem/SKILL.md" && test -d "$repo/agents" && echo "repo: $repo"
```

## Link

For OpenCode, three kinds of link: every skill, every agent prompt, and this repo's `AGENTS.md`. The host discovers all three by convention (`~/.config/opencode/{skills,agents}/` + `AGENTS.md`) — nothing goes in `opencode.json`. Idempotent, and never overwrites a real file:

```sh
host="${XDG_CONFIG_HOME:-$HOME/.config}/opencode"; mkdir -p "$host/skills" "$host/agents"
link() { # link <source> <target> <label> — replace a dead link, skip a real file, skip a deliberate pointer
  if [ -L "$2" ]; then
    if [ ! -e "$2" ]; then ln -sfn "$1" "$2"; echo "replaced $3 (its target was gone)"
    elif [ "$(readlink "$2")" = "$1" ]; then echo "ok       $3"
    else echo "skip     $3 (points elsewhere: $(readlink "$2"))"; fi
  elif [ -e "$2" ]; then echo "skip     $3 (a real file is there — move it aside, then re-run)"
  else ln -s "$1" "$2"; echo "linked   $3"; fi
}
for dir in "$repo"/skills/*/; do [ -f "$dir/SKILL.md" ] || continue
  name="$(basename "$dir")"; link "$repo/skills/$name" "$host/skills/$name" "skills/$name"; done
for file in "$repo"/agents/*.md; do [ -e "$file" ] || continue
  name="$(basename "$file")"; link "$repo/agents/$name" "$host/agents/$name" "agents/$name"; done
link "$repo/AGENTS.md" "$host/AGENTS.md" "AGENTS.md"
```

Another host: link the skills where it reads them and ask the human where its agent prompts go.

## The CLI

The prompts call `tandem-cli` by name; the app bundles it and the cask links it into Homebrew's `bin`:

```sh
command -v tandem-cli || echo "missing — brew install --cask gianniacquisto/tap/tandem"
ln -s "/Applications/Tandem.app/Contents/MacOS/tandem-cli" "$(brew --prefix)/bin/tandem-cli"   # if the link is missing
```

## Duplicate routes

If the host config still lists this checkout under `instructions` or `skills`, the host loads the same files twice. Remove the entries that point at this checkout; leave every other entry alone:

```sh
grep -n "instructions\|skills" "${XDG_CONFIG_HOME:-$HOME/.config}/opencode/opencode.json" 2>/dev/null
```

## Verify

```sh
opencode api agent.list   # builder, consolidator, refiner, reviewer
opencode api skill.list   # grilling, retro, tdd, tandem, tandem-consolidate, tandem-memory, tandem-refine, tandem-setup, tandem-sprint, tandem-tidy, tandem-team, writing-for-agents
tandem-cli doctor         # the skills check reads "opencode configured"
```

A change to a skill or to `AGENTS.md` is picked up by a new session. An agent prompt is read when the server starts — live only after a host restart. Compare `opencode api agent.list` (what the server read) with the file to tell which is live.

## Done when

- The host lists every agent prompt and every skill this repo ships, and `tandem-cli doctor` reads "opencode configured".
- Nothing in the host config points at a path that does not exist, and no file was overwritten.
- The report says what was linked, what was skipped and why, and what is left for the human — a skip is a done setup; silence is not.
