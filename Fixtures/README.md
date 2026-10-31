# Fixtures

Three kinds of fixture live here, and the rule differs by kind.

**Scratch homes** are what the screenshot tool shoots: created on demand, never
committed. Every picture comes from a home under this folder — never
`~/.tandem`, never `~/.tandem-dev`, never anything outside this folder
(`docs/screenshots/README.md`, enforced by `Scripts/shot.sh`):

    mkdir -p Fixtures/screenshots/Home
    TANDEM_HOME=Fixtures/screenshots/Home tandem-cli ticket add --title "Fixture ticket"
    # ...seed the fixture vault the same way, then:

    TANDEM_HOME=Fixtures/screenshots/Home Scripts/shot.sh list
    TANDEM_HOME=Fixtures/screenshots/Home Scripts/shot.sh <window-id> docs/screenshots/<file>.png

The fixture data is written here by the CLI, against this folder — the script
refuses any other home. Nothing under a scratch home is committed.

**The port's homes** are the exception: authored and committed, because both
implementations read them and their shape is part of the port's contract
(`docs/port.md`, Phases 1). `Fixtures/port/Home/` is the contract home (four
session records, two board tickets); `Fixtures/port/Board/` is the 78-ticket
board the Tickets face's pictures are shot on. No run writes into either, and
each README states the rule.

**The port's ordering golden**, `Fixtures/port/ordering.json`, is the drag
arithmetic's one case list: `rust/core/tests/ordering.rs` and
`app/tests/board.test.ts` run the same cases through their own implementation,
so a drop index cannot drift between the two.

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
port's schema is its own, `docs/port.md`, Store), so the rule names it and
compares everything around it. `doctor`'s `app` and `skills` rows read the
machine rather than the home, so a case names them in its `mask` and their
detail is `<environment>`; their names and `ok` still compare. `Scripts/capture-goldens.sh` writes the goldens,
`rust/cli/tests/goldens.rs` replays them.

**The port's agent face**, `Fixtures/port/agent-face/`, is the bundle `setup`
installs from: one skill, one prompt, one `AGENTS.md` and a `VERSION`. Both
implementations are pointed at it with `TANDEM_AGENT_BUNDLE`, so the golden says
what an install does without depending on the machine's app bundle or on this
repo's own skills.
