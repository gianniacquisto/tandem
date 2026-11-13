# The port's fixture home

One `~/.tandem` shape both implementations read (`docs/port.md`, Phases 1).

This home is **content, not scratch**: it is authored here and committed, and
no run writes into it. The binary is pointed at it with
`--home Fixtures/port/Home` (or `TANDEM_HOME=Fixtures/port/Home`) and never at
the real `~/.tandem`.

- `bikeshed/` is the vault: `Index.md`, `projects/<project>/<project>.md`, and
  one `daily/` note.
- `tandem.db` is the store: the two board tickets and the one lane the fixture
  carries, generated, never hand-edited:
  `Scripts/make-fixture-home.sh` writes it from `Fixtures/port/tandem.seed.json`
  (every id and timestamp in the seed, no clock), and
  `rust/core/tests/fixture_db.rs` fails when the committed file is not what the
  generator writes. It is in rollback-journal mode, so a read needs no `-wal`
  or `-shm` beside it.
