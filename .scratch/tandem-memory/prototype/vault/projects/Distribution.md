# Distribution

Getting the Tandem app onto this Mac as an installable artifact. Tracker: `tandem/.scratch/distribution/` — a wayfinder map; this note is the memory of the work. *(Sample note: kept deliberately thin — the shape and style are what this prototype is testing.)*

## Where left off

Floor, bundle, release workflow, and docs are cut as cutover branches (`cutover/u1-floor` … `cutover/u4-docs`); cutover is in flight. Next: run the cutover end to end on this Mac.

## Decisions

- **One artifact, one path** — a single notarized app bundle installed to `/Applications`; no Sparkle, no multiple channels. Distribution stays boring while the app is pre-1.0.
- **Signing happens in the release script, not in the IDE** — one script owns signing, notarization, and stapling, so the flow is reproducible from a clean clone.

## Gotchas

- Notarization rejects a bundle whose embedded helper has a stale code signature — re-sign inner bundles before the outer one, or the staple silently skips.
- A Gatekeeper prompt can appear on first launch even for a notarized app if the user opened the `.app` from a quarantined download; the first-launch note covers it.

## Threads

- Run the cutover on this Mac end to end.
