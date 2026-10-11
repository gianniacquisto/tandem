# 01 - Prototype: how far does the Ligne Claire theme reach?

Type: prototype
Status: claimed
Claimed: 2026-10-11 (agent)

## Question

How far should the Ligne Claire + minimalist flat illustration theme (full brief in the map's Notes) reach into the interface, and which style treatments actually land? Build a cheap, rough, concrete artifact to react to — a throwaway SwiftUI prototype (small package, `prototype/<name>` branch, not the real app) covering **both faces** — Main window and a mock of the themed menu bar popover — rendered in **2–3 variants of theme reach**:

- **(A) Brand-only** — tandem-bike app icon + menu bar glyph, wordmark, muted palette in accents; the interface otherwise stock native-clean.
- **(B) Subtle Ligne Claire** — A plus theme in the chrome: flat-filled surfaces with crisp uniform linework, a line-drawn checkbox, line-drawn filter control, cel-shaded (hard-edge, no-gradient) accents, an illustrated empty state (small tandem bike, vast negative space).
- **(C) Deeper** — B plus illustrated header band (expansive sky, geometric untextured hills, the bike), panel-style list treatment, more illustrated moments.

Hard constraints on every variant (from the brief): muted desaturated earthy palette (dusty blues, washed-out reds, sandy beiges), flat fills, hard-edged shading only, no gradients, generous negative space, native-clean body typography. Present each variant in **light and dark**.

The prototype must also produce **draft assets** to react to, since they gate everything downstream:

1. A Ligne Claire **tandem-bike illustration** — app icon candidate + menu bar template-glyph candidate (the bike is the brand motif: two riders, one machine).
2. **2–3 wordmark treatments** for "Tandem" (custom line lettering vs. a clean display face vs. both) — the 8-bit pixel font is dead; this is the open type question.
3. A **candidate palette** — named, with concrete light + dark values.

**Verdict (record on resolution):** the chosen reach (A/B/C or a hybrid), which treatments/assets land, and what dies. Losing variants stay on the branch; main keeps only the verdict.
