# 03 - Grilling: lock the final brand assets and microcopy

Type: grilling
Status: done
Blocked by: 02
Resolved: 2026-10-11 (one grilling round — the human pointed at the brand asset already in hand and answered all six lock questions)

## Question

With the v2 design settled by the two prototypes, lock the final brand assets and voice so the spec can name them exactly:

1. **Wordmark + display type** — pick from the treatments surfaced in ticket 01; if a font is bundled, verify its license is permissive (OFL or similar) and record it.
2. **Final palette** — exact values for light and dark (dusty blues, washed-out reds, sandy beiges, per the brief), named, with usage rules (which is background, linework, accent, done-state, error).
3. **Tandem-bike illustration set** — final art for: app icon (feeds `Scripts/make-app-icon.sh` → `Resources/AppIcon.icns`), menu bar template glyph, empty-state art (main window + popover). One consistent Ligne Claire drawing, adapted per surface.
4. **Microcopy voice** — calm, slightly wry, cosy: empty states ("All clear" etc.), corrupt-store banner, popover truncation row, quick-add placeholders. Present the full set of strings for approval.

The agent presents concrete finals (rendered, not described); the human approves or adjusts each. **Resolution records** the approved palette values, asset list + locations, wordmark/font decision, and the final string set.

## Resolution (2026-10-11)

The brand asset was already in hand: a tandem-bike photo dropped in the repo base dir. All six lock questions settled in one round:

1. **Brand asset (the logo)** — the tandem-bike photo, committed as `Resources/TandemBike.jpg` (2400×1792). The base dir held two files: the ".png" was a mislabelled JPEG and the `.jpeg` was the keeper; both are now gone, replaced by the single committed asset.
2. **App icon** — regenerated from the photo (old: white checklist SF Symbol on a blue gradient): the **whole frame stays visible (no crop)**, letterboxed on the day-panel paper (0xF5F0E6) rounded plate, same Apple icon grid. `Scripts/make-app-icon.swift` rewritten; `make-app-icon.sh` and the .iconset/.icns pipeline unchanged; `Resources/AppIcon.icns` regenerated. The plate is the light paper — an .icns is one bitmap, so the light plate is the one that reads in both appearances.
3. **In-app surfaces** — the photo replaces the vector bike in the **main window's empty states** (hero 220pt + small 96pt), framed with a sand hairline + corner rounding so it reads as deliberate on the paper. The **menu bar template glyph and the popover header mark keep the vector bike**: a colour photo can't be a template image, and a 24pt photo mark is a smudge. `Bike.swift` stays for those two surfaces.
4. **Loading** — the bundle ships `Contents/Resources/TandemBike.jpg` (`Scripts/make-app-bundle.sh` copies it); `BrandAsset` loads via `Bundle.main` with a source-checkout fallback for `swift run`, and the vector scene is the fallback art if the asset is ever missing.
5. **Wordmark, palette, microcopy** — stand as implemented: Avenir Next Demi Bold wordmark, the `LigneTheme.swift` day/night palette (now the locked values), and the running app's string set (approved with the ticket 02 build). No changes requested.
