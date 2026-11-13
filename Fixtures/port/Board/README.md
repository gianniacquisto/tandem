# The port's board home

A second committed home, the one the board fixture is drawn from.

This home is **content, not scratch**: it is authored here and committed, and no
run writes into it. The binary is pointed at it with `TANDEM_HOME` and never at
the real `~/.tandem`.

- `tandem.db` is the store: 78 tickets over four lanes (Port, Tandem, Retro,
  Fleet) plus the default area, every status, the archive included, named
  assignees, and three dependency edges so the blocked mark is real. It holds
  the seven tables the cut kept, not a usage report.
- It is generated, never hand-edited: `Scripts/make-fixture-home.sh
  Fixtures/port/board.seed.json Fixtures/port/Board/tandem.db` writes it from
  `Fixtures/port/board.seed.json` (every id and timestamp in the seed, no
  clock), and `rust/core/tests/fixture_db.rs` fails when the committed file is
  not what the generator writes.
- It is in rollback-journal mode, so a read needs no `-wal` or `-shm` beside it.
