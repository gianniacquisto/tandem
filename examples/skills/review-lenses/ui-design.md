# UI design lens

The checklist for a diff that draws or sizes something in the UI. It reads the templates and the CSS, plus any screenshot the PR attaches, and judges no screen it cannot see.

## Read

- Every literal number in a width, height, gap, padding or radius against the repo's design tokens.
- Every colour: a token, or a literal that bypasses the light and dark blocks.
- Type: font choices against the theme's own, and whether the change holds at the sizes the UI already uses.
- Dark mode, where the diff's view appears in both palettes.
- Popover and sheet sizing: the height and width a view asks for against its content, and whether a long title or an empty list changes it.
- A new component or screen against the repo's design reference, where the tokens are silent. The tokens win over it when they disagree. A diff reads tokens only, and a new token lands in every palette the UI ships, with an AA check (4.5:1 text, 3:1 UI) in each.
- The markup contract: a template change keeps the ids and classes the client script names, or moves both in the same diff.

## Blocks on

- A spacing, radius, line or colour literal where the tokens have one, so one view drifts from the rest of the UI.
- A colour that reads only in one palette, when the tokens carry both.
- A fixed frame on a control that truncates its own text at a length the data allows.
- A screenshot the PR attaches that shows the token broken (a clipped label, a colour that fails in dark mode), judged as the diff's own change.

## Never blocks on

- One composition over another that both use the tokens.
- A pixel of rhythm the reviewer would have placed differently.
- A screen the diff does not touch and the PR does not show.
