# The editor gains a formatting toolbar and a persisted font size

The bikeshed editor is one HTML page (ticket 07): a CM6 inline-live preview, a Ligne theme, and the Swift host. Two of its standing gripes are addressed at once — *"need options to control font and add checkboxes etc"* and *"bikeshed needs to be clean — better formatting options like Obsidian."* We decided: the page gains a **quiet formatting toolbar** above the note (Bold, Italic, Strikethrough, Inline code, Link, Heading, Bullet list, Numbered list, Task list, Quote, Code block) and a **font-size control** (`A− size A+`, 11–20 px, step 1). Both are **page-side** — part of the editor bundle, rendered by `Editor/src/toolbar.js`, not SwiftUI chrome. Every button dispatches an **ordinary CM6 transaction** built by pure spec functions in `main.js`, so **raw markdown stays the source of truth** and the autosave/conflict machinery ([ADR 0004](0004-vault-protocol.md)) is untouched. The font size sets the CSS custom property `--tandem-editor-font-size` on the editor root and persists across launches via the bridge in `UserDefaults`.

## Why the toolbar is page-side, not SwiftUI chrome

- **It must travel with the editor it edits.** The buttons read the CM6 selection and dispatch into the CM6 view; splitting that across a web process boundary would make every keystroke-scale action a round trip for no gain.
- **One home for the editor's surface.** index.html/editor.js/style.css are the whole editor page; a SwiftUI bar above it would be a second place to keep the editor's vocabulary. The bridge stays what it is — document in, edits out — not a control channel.
- **The SwiftUI pane is just a container** (`BikeEditorPane` embeds the persistent `WKWebView`). Adding chrome there would also drag the toolbar into window layout questions the page already answers with flexbox.

## Why formatting is ordinary transactions

- **The buffer is the document.** Each action is a `TransactionSpec` (`wrapSelectionSpec`, `toggleLinePrefixSpec`, `cycleHeadingSpec`) that reads `state.doc`/`state.selection` and returns changes — the same shape as `insertTextSpec`/`setDocumentSpec`. There is no second representation to keep in sync.
- **Nothing downstream changes.** A formatting edit trips the same `dirty` listener as typing, so autosave and the conflict banner behave exactly as before; a spec is provably testable headlessly on a plain `EditorState`, which is how the toolbar's behaviour is pinned.
- **Block formatting is enough; checkboxes already work.** The preview's task checkboxes are already interactive (clicking one dispatches the flip). The **Task-list button only inserts the `- [ ]` syntax** — it is a convenience, not a new interaction.

## Why font size rides the bridge into `UserDefaults`

- **It is a preference, not document state.** Font size must not touch the note or its autosave; it is a view setting that outlives the session. The page posts `{type:"font-size", value}`, the host stores the integer, and on `loaded` the host sends `setFontSize(px)` back — the same one-way pattern as `setTheme`.
- **It scales the whole pane from one property.** `theme.js`'s root rule reads `var(--tandem-editor-font-size)` instead of a literal, so headings, code, and tables scale together. The toolbar's own chrome deliberately does **not** use the property, so it stays a fixed small size.
- **The host owns the store, the page owns the paint.** Persistence is a host concern (`UserDefaults`), so a fresh page — reloaded on the next launch — gets the value pushed back at `loaded`, exactly as the theme does. The page never reads or writes a file, which keeps the no-file/no-network invariant of the bundle intact.

## Why "clean" meant restraint, not a redesign

- **The note typography is not touched.** "Clean" is answered by the toolbar's *register*: a hairline `--sand` bottom border, `--paper`/`--parchment` surfaces, `--ink-dim` line-drawn glyphs, no filled or saturated buttons — the Ligne register, not a new one. The toolbar is the only visible addition above the note.

## Rejected

- **SwiftUI toolbar.** A second place for editor vocabulary and a round trip per action (see above).
- **A rendered/WYSIWYG editing model.** Formatting would stop being ordinary markdown edits and the source-of-truth invariant would have to be renegotiated.
- **A rich font menu (family, weight, per-note sizes).** The ask is "control font"; one pane-wide size covers it without putting a preference in the note.
- **Letting the toolbar scale with the note font.** It would grow into the note's space; chrome stays chrome-sized.
- **A Markdown shortcut layer over the toolbar's actions.** Modifier-key bindings are a separate feature with their own discoverability problem; the toolbar is the visible affordance, and the actions being pure specs means bindings can call the same functions later without re-deciding anything here.

## Consequences

- The fonts/typography defaults and invariants of the note are unchanged; only the toolbar and the root font-size property are new.
- The bridge contract grows by one message (`font-size`) and one page global (`setFontSize`); `index.html`'s bridge comment and `bridge.js`'s header list both name them.
- The persisted size lives under a single stable `UserDefaults` key; an unset value yields the 13.5 px default in `style.css`.
- **On the shelf:** this reverses no earlier scope cut — it extends the editor's feature list, and the editor remains "raw markdown is the source of truth." It is adjacent to [ADR 0009](0009-cli-agent-face.md) only in that both keep the notes interface (file / buffer) primary and add convenience around it; neither changes the file format, the lock, or the agent surface.
