#!/usr/bin/env bash
# Wire this checkout into an agent host. Idempotent; never overwrites a real file.
#
# It links three things into the host's config directory:
#   - the skills in skills/        -> <host>/skills/<name>
#   - the agent prompts in agents/ -> <host>/agents/<name>.md
#   - this repo's AGENTS.md        -> <host>/AGENTS.md
#
# Every link points back into this checkout, so the live prompt is the file in
# the tree: edit here, and the host sees it. A checkout on a branch without
# those files leaves the links dangling — that is the "dangling" report below.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${XDG_CONFIG_HOME:-$HOME/.config}/opencode"
out() { printf '  %s\n' "$*"; }

link() { # link <source> <target> <label>
  local src=$1 target=$2 label=$3
  mkdir -p "$(dirname "$target")"
  if [ -L "$target" ]; then
    if [ ! -e "$target" ]; then
      out "dangling $label (its target is missing — is this checkout on a branch that has it?)"
      return
    fi
    if [ "$(readlink "$target")" = "$src" ]; then
      out "ok       $label"
    else
      out "skip     $label (symlink to elsewhere: $(readlink "$target"))"
    fi
    return
  fi
  if [ -e "$target" ]; then
    out "skip     $label (real file exists — move it aside and re-run)"
    return
  fi
  ln -s "$src" "$target"
  out "linked   $label"
}

echo "tandem agent host"

if [ ! -d "$HOST" ]; then
  echo "  no host config at $HOST — create it, or point XDG_CONFIG_HOME at the one you use."
  exit 1
fi

for dir in "$HERE"/skills/*/; do
  [ -f "$dir/SKILL.md" ] || continue
  name="$(basename "$dir")"
  link "$HERE/skills/$name" "$HOST/skills/$name" "skills/$name"
done

for file in "$HERE"/agents/*.md; do
  [ -e "$file" ] || continue
  name="$(basename "$file")"
  link "$HERE/agents/$name" "$HOST/agents/$name" "agents/$name"
done

link "$HERE/AGENTS.md" "$HOST/AGENTS.md" "AGENTS.md"

echo
if command -v tandem-cli >/dev/null 2>&1; then
  echo "tandem-cli: $(command -v tandem-cli)"
else
  echo "tandem-cli: not on PATH — install the app (brew install --cask gianniacquisto/tap/tandem) and link its binary:"
  echo "  ln -s \"/Applications/Tandem.app/Contents/MacOS/tandem-cli\" \"\$HOME/.local/bin/tandem-cli\""
fi

cat <<'NOTE'

If your host config (opencode.json) still lists this checkout's AGENTS.md under
"instructions" or its skills/ under "skills", drop those entries: the links above
already carry the same files, and both routes would load them twice.
NOTE
