# Fixtures

Two kinds of fixture home live here, and the rule differs by kind.

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

**The port's home**, `Fixtures/port/Home/`, is the exception: it is authored and
committed, because both implementations read it and its shape is part of the
port's contract (`docs/port.md`, Phases 1). No run writes into it. Its README
states the rule.
