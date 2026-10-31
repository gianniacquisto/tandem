# UI design lens

The checklist for a diff that draws or sizes something in `app/renderer/`. It reads the React source and any screenshot the PR attaches, and judges no screen it cannot see.

## Read

- Every literal number in a width, height, gap, padding or radius against the tokens in `app/renderer/src/tokens.css`: `--space-gutter`, `--space-row`, `--space-control`, `--space-lane`, `--radius-card`, `--hairline`.
- Every colour: a token from `app/renderer/src/tokens.css`, or a literal that bypasses the day and night blocks.
- Type: font choices against the theme's own, and whether the change holds at the sizes the app already uses.
- Dark mode, where the diff's view appears in both palettes.
- Popover and sheet sizing: the height and width a view asks for against its content, and whether a long ticket title or an empty list changes it.

## Blocks on

- A spacing, radius, line or colour literal where `app/renderer/src/tokens.css` has the token, so one view drifts from the rest of the app.
- A colour that reads only in one palette, when `tokens.css` carries both.
- A fixed frame on a control that truncates its own text at a title length the store allows.
- A screenshot the PR attaches that shows the token broken (a clipped label, a colour that fails in dark mode), judged as the diff's own change.

## Never blocks on

- One composition over another that both use the tokens.
- A pixel of rhythm the reviewer would have placed differently.
- A screen the diff does not touch and the PR does not show.
