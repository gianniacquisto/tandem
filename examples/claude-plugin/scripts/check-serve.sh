#!/bin/sh
# SessionStart check for the Tandem plugin (ADR 0051). Silent when all is
# well. Otherwise prints one line per finding, which Claude Code adds to the
# session's context so the agent can tell the human:
#
#   - no `tandem serve` answers, so the Tandem tools are missing;
#   - the running server is a different version from the installed binary,
#     so it predates an upgrade and wants a restart;
#   - a newer Tandem release is out than the one installed.
#
# The release check asks GitHub at most once a day and caches the answer in
# ${XDG_CACHE_HOME:-~/.cache}/tandem/latest-release. TANDEM_NO_UPDATE_CHECK=1
# turns it off. Never fails the session: every path exits 0.

base="${TANDEM_URL:-http://127.0.0.1:7700}"
install_cmd="curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh -s -- --reinstall"

command -v curl >/dev/null 2>&1 || exit 0

installed=""
if command -v tandem >/dev/null 2>&1; then
  installed=$(tandem --version 2>/dev/null | awk '{print $2}')
fi

# 1. Is the server up?
if ! curl -fsS --max-time 2 "$base/health" >/dev/null 2>&1; then
  if [ -n "$installed" ]; then
    echo "Tandem: no server answered at $base, so the Tandem MCP tools are unavailable this session. Ask the human to run \`tandem serve\` in a terminal and leave it running, then reconnect the server with /mcp."
  else
    echo "Tandem: the \`tandem\` binary is not on PATH and no server answered at $base. Install it with: curl -fsSL https://github.com/gianniacquisto/tandem/releases/latest/download/install.sh | sh, then run \`tandem serve\` and reconnect with /mcp."
  fi
  exit 0
fi

# 2. Does the running server match the installed binary? A token-guarded
# server answers / with its login page, which carries the version too; when
# no version is found the check is skipped.
serving=$(curl -fsS --max-time 2 "$base/" 2>/dev/null |
  sed -n 's/.*data-version="\([^"]*\)".*/\1/p' | head -n 1)
if [ -n "$installed" ] && [ -n "$serving" ] && [ "$serving" != "$installed" ]; then
  echo "Tandem: the running server is version $serving but the installed binary is $installed. Ask the human to restart \`tandem serve\` so the board and tools match the install."
fi

# 3. Is a newer release out?
[ "${TANDEM_NO_UPDATE_CHECK:-}" = "1" ] && exit 0
[ -n "$installed" ] || exit 0

cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/tandem"
cache="$cache_dir/latest-release"
latest=""
if [ -f "$cache" ] && [ -z "$(find "$cache" -mmin +1440 2>/dev/null)" ]; then
  latest=$(cat "$cache" 2>/dev/null)
else
  latest=$(curl -fsSI --max-time 2 https://github.com/gianniacquisto/tandem/releases/latest 2>/dev/null |
    tr -d '\r' | sed -n 's|^[Ll]ocation: .*/releases/tag/v\([0-9][0-9.]*\)$|\1|p' | head -n 1)
  if [ -n "$latest" ]; then
    mkdir -p "$cache_dir" 2>/dev/null && printf '%s\n' "$latest" >"$cache" 2>/dev/null
  fi
fi
[ -n "$latest" ] || exit 0

# Numeric X.Y.Z comparison: prints 1 when $1 is older than $2.
older=$(awk -v a="$installed" -v b="$latest" 'BEGIN {
  split(a, x, "."); split(b, y, ".");
  for (i = 1; i <= 3; i++) { if (x[i] + 0 < y[i] + 0) { print 1; exit } if (x[i] + 0 > y[i] + 0) { print 0; exit } }
  print 0 }')
if [ "$older" = "1" ]; then
  echo "Tandem: release $latest is out and $installed is installed. To upgrade: $install_cmd, then restart \`tandem serve\`, and bring the plugin to the same release: /plugin marketplace update tandem, then /plugin uninstall tandem@tandem and /plugin install tandem@tandem."
fi
exit 0
