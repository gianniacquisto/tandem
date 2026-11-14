# UI design lens

The checklist for a diff that draws or sizes something in the served board. It reads the Askama templates under `rust/core/templates/` and the CSS under `rust/core/src/serve/static/`, plus any screenshot the PR attaches, and judges no screen it cannot see.

## Read

- Every literal number in a width, height, gap, padding or radius against the tokens in `rust/core/src/serve/static/tokens.css`: `--space-gutter`, `--space-row`, `--space-control`, `--space-lane`, `--radius-card`, `--hairline`.
- Every colour: a token from `rust/core/src/serve/static/tokens.css`, or a literal that bypasses the day and night blocks.
- Type: font choices against the theme's own, and whether the change holds at the sizes the board already uses.
- Dark mode, where the diff's view appears in both palettes.
- Popover and sheet sizing: the height and width a view asks for against its content, and whether a long ticket title or an empty list changes it.
- A new component or screen against `DESIGN.md` at the repo root, where `tokens.css` is silent. The tokens win over it when they disagree. The board has four palettes (`neutral` and `tandem`, each light and dark, selected by `data-style` and `data-theme` on `<html>`): a diff reads tokens only, and a new token lands in all four sets with an AA check (4.5:1 text, 3:1 UI) in each.
- The markup contract: a template change keeps the ids and classes `tandem.js` and the htmx swaps name, or moves both in the same diff.

## Blocks on

- A spacing, radius, line or colour literal where `rust/core/src/serve/static/tokens.css` has the token, so one view drifts from the rest of the board.
- A colour that reads only in one palette, when `tokens.css` carries both.
- A fixed frame on a control that truncates its own text at a title length the store allows.
- A screenshot the PR attaches that shows the token broken (a clipped label, a colour that fails in dark mode), judged as the diff's own change.

## Never blocks on

- One composition over another that both use the tokens.
- A pixel of rhythm the reviewer would have placed differently.
- A screen the diff does not touch and the PR does not show.
