# Fixtures

The fixtures live here, and the rule differs by kind.

**Scratch homes** are the throwaway copies a manual look opens, and no scratch
home is committed. A case copies a home under this folder to a scratch
directory and points the server at the copy (`TANDEM_HOME=<scratch>/Home`), so
no run writes into the fixture and nothing opens `~/.tandem`. The screenshot rig
that used to copy these homes is cut (CUT-2); this folder only holds the homes
the loop reads.

**The port's homes** are the exception: authored and committed, because both
`tandem serve` and the CLI read them and their shape is part of the fixture
contract. `Fixtures/port/Home/` is the contract home (two board tickets and one
lane); `Fixtures/port/Board/` is the 78-ticket board the board fixture is drawn
from. No run writes into either, and each README states the rule.

**The port's ordering golden**, `Fixtures/port/ordering.json`, is the drag
arithmetic's one case list: `rust/core/tests/ordering.rs` runs the cases through
the port's implementation, so a drop index cannot drift between the board and
the notes tree.

**The port's goldens**, `Fixtures/goldens/`, are the CLI's contract written down:
`manifest.json` is the case list, one deliberate line per verb shape, and
`<group>/<case>.json` is what the Swift binary answered for it. The manifest is
the single home of the case, the invocation and the normalization rules; a golden
is only the result, self-describing enough that a reviewer can read it alone:

    argv, stdin      what was run, copied from the manifest (the Rust test fails
                     when the two disagree, so a golden cannot go stale quietly)
    exit             the code the run returned
    stdoutKind       "json" when the answer parses as a JSON object or array,
                     "text" otherwise
    stdout           that JSON, or one entry per line for text
    stderr           one entry per line, always

Ids, timestamps, the two sandbox paths and the store's schema version appear as
`<id>`, `<timestamp>`, `<tandem-home>`, `<home>` and `<schema>`, and nothing else
is normalized. The schema is the one number the two stores do not share (the
port's schema is its own), so the rule names it and compares everything around
it. `doctor`'s rows read the sandbox home and the fixture's paths, and the
normalization names them; their names and `ok` still compare. The goldens were
written by the Swift capture before the cutover, which retired with the tree;
`rust/cli/tests/goldens.rs` replays them, and a new shape is a deliberate
manifest line plus a file written to the same shape.

**The port's agent face**, `Fixtures/port/agent-face/`, is a historical fixture:
the bundle the retired `setup` verb installed from, kept because the goldens'
environment still pins `TANDEM_AGENT_BUNDLE`. The bundled face and its install
verb left the tree with CUT-3 ([ADR 0045](../docs/adr/0045-the-board-and-notes-cut.md)).
