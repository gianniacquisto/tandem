# Tandem v2 UI prototype (throwaway)

Answers ticket 01 (v2 map): **"How far does the Ligne Claire theme reach?"**

Three *reach* variants of the themed app, both faces, in light and dark, plus
the draft assets everything downstream gates on. **Run:**

```sh
cd prototype && swift run TandemPrototype
```

The floating bottom bar (and `←`/`→`) cycles:

- **Reach** — `A · Brand-only` / `B · Subtle` / `C · Deeper`
- **Face** — `Main window` / `Popover` / `Assets`
- **Appearance** — `Auto` / `Light` / `Dark` (day panel / night panel)
- `banner` — demo the restyled corrupt-store banner (Main face)
- `reset data` — restore the seed (the v2 checkbox is one-way, by decision)

## Reach variants (the question)

- **A — Brand-only**: stock native-clean interface; the theme lives only in
  the bike icon, wordmark, and accents.
- **B — Subtle Ligne Claire**: theme in the chrome — paper surfaces, uniform
  linework, line-drawn checkbox and filter control, cel-shaded (hard-edge,
  no-gradient) accents, illustrated empty state.
- **C — Deeper**: B + illustrated header band (expansive sky, geometric
  untextured hills, the tandem), panel-style list treatment (flat border +
  hard cel offset), more illustrated moments.

The **menu bar popover** is a real `NSStatusItem` + themed `NSPopover` (top
right of the screen) and is *constant across reach variants* — the map
settled that the menu bar is the most-visible brand surface and wears the
theme. The `Popover` face shows the same content as an on-screen mock.

## Draft assets (the `Assets` face; PNGs in `preview/`)

1. **Tandem-bike illustration** — two riders, one machine; feeds the app
   icon, the menu bar template glyph (bare bike — the double saddle is the
   tandem tell), and the empty states.
2. **App icon candidate** — flat sky, sun/moon, two hills, the tandem.
3. **Wordmark treatments** — 1: custom line lettering · 2: display face
   (Avenir Next Demi Bold) · 3: hybrid (face + bike riding its baseline).
4. **Candidate palette** — named, with day + night values and usage rules.

Export the PNGs with:

```sh
cd prototype && swift run TandemPrototype --export-previews
```

## Decisions this prototype surfaces (all still open)

1. **Interface reach** — A / B / C or a hybrid (e.g. "B's window, C's band").
2. **Which treatments/assets land** — checkbox, filter, banner, empty state,
   header band, panel; and what dies.
3. **Wordmark direction** — line lettering, display face, or hybrid; which
   display face.
4. **Palette** — the named candidate values and usage rules.

## Caveats

In-memory seed data only — no store, no persistence, no MCP. The popover
arrow in the on-screen mock is a flat stand-in for the `NSPopover` chrome.
This is a throwaway for judging look; it is not the app.
