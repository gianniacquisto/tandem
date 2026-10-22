#!/usr/bin/env bash
# Wire TERSE.md into every harness found on this machine. Idempotent; never overwrites.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC="$HERE/TERSE.md"
out() { printf '  %s\n' "$*"; }

link() { # link <source> <target> <label>
  local src=$1 target=$2 label=$3
  if [ -L "$target" ]; then
    if [ ! -e "$target" ]; then
      out "dangling $label (its target is missing — is this repo on dev?)"
      return
    fi
    if [ "$(readlink "$target")" = "$src" ]; then
      out "ok       $label"
    else
      out "skip     $label (symlink to elsewhere)"
    fi
    return
  fi
  if [ -e "$target" ]; then
    out "skip     $label (real file exists — merge TERSE.md manually)"
    return
  fi
  mkdir -p "$(dirname "$target")"
  ln -s "$src" "$target"
  out "linked   $label"
}

retire() { # retire <target> <why>
  local target=$1 why=$2
  if [ -L "$target" ]; then
    rm "$target"
    out "removed  $target ($why)"
  fi
}

echo "terse"

# Claude Code: user-level rules load in every session, every project.
if [ -d "$HOME/.claude" ]; then
  link "$SRC" "$HOME/.claude/rules/terse.md" "~/.claude/rules/terse.md"
fi

# Codex: one global instruction file, no import syntax.
if [ -d "$HOME/.codex" ]; then
  link "$SRC" "$HOME/.codex/AGENTS.md" "~/.codex/AGENTS.md"
fi

# OpenCode: the plugin owns the injection, because it has to be able to stop.
if [ -d "$HOME/.config/opencode" ]; then
  link "$HERE/opencode/plugin" "$HOME/.config/opencode/plugins/terse" "~/.config/opencode/plugins/terse"
  link "$HERE/opencode/terse.sh" "$HOME/.config/opencode/terse.sh" "~/.config/opencode/terse.sh"
  link "$HERE/opencode/commands/terse.md" "$HOME/.config/opencode/commands/terse.md" "~/.config/opencode/commands/terse.md"
  # terse used to be a selectable primary agent; the switch replaced it.
  retire "$HOME/.config/opencode/agents/terse.md" "the terse agent is gone — terse is a switch"
fi

# With the plugin installed the config entry is a second, always-on copy of the same file.
cfg=""
for candidate in "$HOME/.config/opencode/opencode.jsonc" "$HOME/.config/opencode/opencode.json"; do
  [ -f "$candidate" ] && cfg=$candidate && break
done
if [ -n "$cfg" ]; then
  if grep -q "TERSE.md" "$cfg"; then
    out "manual   remove the TERSE.md path from \"instructions\" in $cfg (the plugin injects it now)"
  else
    out "ok       opencode.json instructions"
  fi
fi

echo
echo "switch: $("$HERE/opencode/terse.sh") — /terse off or $HOME/.config/opencode/terse.sh off"
echo "restart a running session to pick up a newly linked plugin."
