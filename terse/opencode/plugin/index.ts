// The terse behaviour, as a switch rather than a mode you escape from.
//
// OpenCode's `instructions` config is always on and only config edits change it, which made
// the style a room you have to leave. This plugin instead adds TERSE.md to the system prompt of
// each model request while the switch says on, and adds nothing when the switch says off. Off is
// the style genuinely absent, not a second instruction arguing with the first.
//
// Scope: every OpenCode agent and every session at this location, including sub-agent children,
// because the hook is registered per session at plugin load and runs for each request.
//
// The switch is one file, readable and writable by hand: ~/.config/opencode/terse.json.
// Missing means on, so a fresh machine behaves as before.

import { readFileSync, realpathSync } from "node:fs"
import { homedir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

// The style lives beside the plugin: <repo>/terse/TERSE.md, two directories up from this file.
// Resolved from this file's own location — through any symlink the host put here — so the repo
// can be cloned anywhere and no path in this file names one machine.
const PLUGIN_DIR = realpathSync(dirname(fileURLToPath(import.meta.url)))
const DEFAULT_STYLE = resolve(PLUGIN_DIR, "..", "..", "TERSE.md")
const SWITCH = join(process.env.XDG_CONFIG_HOME ?? join(homedir(), ".config"), "opencode", "terse.json")

/** On unless the switch file says otherwise. An unreadable switch is not a reason to go quiet. */
function enabled(): boolean {
  try {
    return JSON.parse(readFileSync(SWITCH, "utf8")).enabled !== false
  } catch {
    return true
  }
}

function style(path: string): string | undefined {
  try {
    const text = readFileSync(path, "utf8").trim()
    return text.length > 0 ? text : undefined
  } catch {
    return undefined
  }
}

/** Append to the system prompt without breaking the parts OpenCode already assembled. */
function attach(event: any, text: string) {
  const parts = event?.system
  if (!Array.isArray(parts)) return
  const last = parts.map((part: any, i: number) => [part, i]).filter(([part]: any) => part?.type === "text").pop()
  if (!last) {
    parts.push({ type: "text", text })
    return
  }
  const [part, i] = last
  parts[i] = { ...part, text: `${part.text}\n\n${text}` }
}

export default {
  id: "terse",
  async setup(ctx: any) {
    const path: string = ctx.options?.style ?? DEFAULT_STYLE
    const handler = (event: any) => {
      if (!enabled()) return
      const text = style(path)
      if (text) attach(event, text)
    }
    // Only the agent loop. A style meant for a human reading a reply has no business in a title
    // or a compaction summary, and those requests should not pay its tokens.
    await ctx.session.hook("context", handler)
  },
}
