# Obsidian wikilink semantics — researched 2026-10-14

Primary: help.obsidian.md (fetched via its publish host raw `.md`). Community (Obsidian forum, incl. ex-dev "WhiteNoise" posts) where primary is silent. "Official docs silent" = not stated in current help.

## 1. Case sensitivity
- Official docs silent on case sensitivity of link resolution.
- Community: wikilink resolution is **case-insensitive** — `[[my note]]` resolves to `My note.md` (app-level index matching, not filesystem; no platform caveat given in sources):
  - https://forum.obsidian.md/t/rename-twice-standardizing-internal-links-future-proofing-wikilinks/66131 ("Obsidian is case-insensitive for internal links")
  - https://forum.obsidian.md/t/maintain-casing-of-text-when-using-link-autosuggestion/23041 (WhiteNoise: differently-cased links "already point to the same note")
  - https://trip2g.com/dev/obsidian_links (algorithm writeup: "Search by filename (case-insensitive)")

## 2. Duplicate basenames
- `[[folder/Name]]` path is **relative to the vault root**, forward slashes on all OSes: https://help.obsidian.md/links ("Folder paths start at the vault root")
- Bare `[[Name]]` with duplicates: not unresolved — Obsidian resolves to **one** of the matches. Community: shortest path from vault root wins (closest to root): https://trip2g.com/dev/obsidian_links
- Disambiguation: write a path-qualified link `[[folder/Name]]` or add YAML `aliases`. Autocomplete inserts the path when the name isn't unique: https://forum.obsidian.md/t/different-behaviour-in-naming-with-wikilinks-vs-md-links-when-referencing-files-in-folders/45770
- With "Shortest path when possible" + auto-update on, existing ambiguous bare links get **rewritten to include the vault-root path**: https://forum.obsidian.md/t/does-obsidian-automatically-look-for-ab-png-in-b-png-in-a-md/39450

## 3. "New link format" setting
- Setting: Settings → Files and links → **New link format** (what links to insert when auto-generating internal links). Options: **Shortest path when possible**, **Relative path to file**, **Absolute path in vault**: https://help.obsidian.md/settings
- Default: "Use [[Wikilinks]]" is on by default ("By default… Obsidian generates links using the Wikilink format"): https://help.obsidian.md/links . Default path option is "Shortest path when possible" (docs don't state; community-observed default): https://forum.obsidian.md/t/settings-new-link-format-what-is-shortest-path-when-possible/6748
- Exact text written (wikilinks on): unique name → `[[Name]]`; duplicate name → vault-root path `[[folder/Name]]` (dev-confirmed "absolute path from the vault root" for non-unique): https://forum.obsidian.md/t/settings-new-link-format-what-is-shortest-path-when-possible/6748 ; relative mode → `[[../folder/Name]]` relative to the containing note; absolute mode → `[[folder/Name]]` full path. `.md` omitted in wikilinks: https://help.obsidian.md/links
- Wikilinks off: Markdown links instead, e.g. `[Name](Name.md)` / `[Name](folder/Name.md)` with `.md` and URL-encoded spaces; `![[images]]` become Markdown images: https://help.obsidian.md/settings

## 4. Broken links
- Render with a **more muted color** to indicate the note doesn't exist: https://help.obsidian.md/link-notes
- Clicking a broken link **creates the note** (no confirm dialog documented; docs: "Create the note by clicking on the link while pressing Ctrl/Cmd"): https://help.obsidian.md/link-notes
- Path-qualified `[[folder/missing]]`: note is created **at that folder path**, overriding "Default location for new notes": https://help.obsidian.md/links and https://help.obsidian.md/settings ("creating `[[Projects/Three laws of motion]]` creates the note in the `Projects` folder relative to the vault root")

## 5. Rename/move of a note
- "When you rename a file, Obsidian **automatically updates all the links** to that file": https://help.obsidian.md/manage-notes
- Controlling setting: Settings → Files and links → **Automatically update internal links**. Off → you are **prompted** after renaming (prompt offers update / "Do not update"): https://help.obsidian.md/settings ; https://forum.obsidian.md/t/links-do-not-update-when-renaming-a-file/80455
- Markdown-style links `[text](Note.md)`: they are the equivalent internal-link format, covered by "all links"/"internal links" updates: https://help.obsidian.md/links
- Embeds `![[...]]`: embed = internal link with `!` prefix, so they update too; "Use Wikilinks" setting explicitly covers `![[images]]`: https://help.obsidian.md/embeds ; https://help.obsidian.md/settings
- Case-only renames: links **are** updated (case normalized) — basis of the "rename twice" trick: https://forum.obsidian.md/t/rename-twice-standardizing-internal-links-future-proofing-wikilinks/66131
- Caveats: renames done outside Obsidian (or while closed) do not update links: https://forum.obsidian.md/t/links-do-not-update-when-renaming-a-file/80455 ; in "Relative path" mode, moving the *containing* note does not yet update its relative links (dev: "to-do"): https://forum.obsidian.md/t/renaming-file-removes-path-from-links-to-it-when-new-link-format-is-shortest/106794

## 6. Syntax
- Alias/display text `[[note|display]]`: supported (also `[[note#Heading|display]]`): https://help.obsidian.md/links
- Heading `[[note#Heading]]`, chained subheadings `[[note#A#B]]`, same-note `[[#Heading]]`: https://help.obsidian.md/links
- Block refs `[[note#^id]]` (human-readable or hex ids; Obsidian-specific, not standard Markdown): https://help.obsidian.md/links
- Embed `![[note]]` (also `![[note#Heading]]`, `![[note#^id]]`, `![[img|WxH]]`): https://help.obsidian.md/embeds
- Nonexistent heading target: official docs silent. Community: link just opens the note (no scroll); a heading embed whose heading is gone "breaks that link entirely" (no section shown): https://forum.obsidian.md/t/transclusion-links-break-when-sections-renamed/48767

## 7. Path-qualified links on move/rename
- **Yes** — existing links to a moved/renamed file are rewritten, regardless of how the path was written; and the rewritten link is normalized to the current "New link format" (e.g. in "Shortest" mode the folder part is stripped when the name is unique — reported as buggy but is the link-update mechanism working): https://forum.obsidian.md/t/renaming-file-removes-path-from-links-to-it-when-new-link-format-is-shortest/106794
- Same for the vault-root-path disambiguation case (ambiguous bare links gain/lose the path as uniqueness changes): https://forum.obsidian.md/t/does-obsidian-automatically-look-for-ab-png-in-b-png-in-a-md/39450
