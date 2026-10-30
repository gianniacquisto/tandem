# The port's fixture home

One `~/.tandem` shape both implementations read (`docs/port.md`, Phases 1).

This home is **content, not scratch**: it is authored here and committed, and
no run writes into it. The Rust binary is pointed at it with
`--home Fixtures/port/Home` (or `TANDEM_HOME=Fixtures/port/Home`) and never at
the real `~/.tandem`, which the shipped Swift app owns until cutover
(`docs/port.md`, How the work lands).

The store arrives with PRT-4: this ticket commits the folder and its bikeshed,
not `tandem.db`.

- `bikeshed/` — the vault: `Index.md`, `projects/<project>/<project>.md`, and
  one `daily/` note.
