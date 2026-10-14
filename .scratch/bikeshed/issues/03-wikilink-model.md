# 03 - Wikilink model: resolution, broken links, rename

Type: grilling
Status: open

## Question

Pin down the `[[wikilink]]` model the bikeshed editor implements, so the spec has one normative definition:

1. **Resolution**: by filename (basename, case-sensitive or not?) or by relative path? Duplicate basenames in different folders — allowed, and how disambiguated (path-qualified syntax, first match, error)?
2. **Syntax**: plain `[[note]]` only, or also aliases `[[note|display text]]` and heading targets `[[note#Heading]]`?
3. **Broken links**: how do they render in live preview, and what does a click do (create a note with that name? nothing? a hint)?
4. **Rename**: renaming a note is a file rename. Do wikilinks in *other* notes update to follow (Obsidian rewrites references — a multi-file write)? If yes, define exactly: which files are touched, in what order, and what happens if the rewrite is interrupted.
5. **Where links live**: wikilinks render inline in live preview; in plain source mode they are just text — confirm that is the model (no link UI in source mode).

The answer is the normative wikilink section of the spec.
