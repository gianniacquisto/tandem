# Fixtures

Scratch homes the screenshot tool may shoot. The rule (`docs/screenshots/README.md`,
enforced by `Scripts/shot.sh`): every picture comes from a home under this
folder — never `~/.tandem`, never `~/.tandem-dev`, never anything outside this
folder.

A screenshot home is created on demand and is working state, not content:

    mkdir -p Fixtures/screenshots/Home
    TANDEM_HOME=Fixtures/screenshots/Home tandem-cli ticket add --title "Fixture ticket"
    # ...seed the fixture vault the same way, then:

    TANDEM_HOME=Fixtures/screenshots/Home Scripts/shot.sh list
    TANDEM_HOME=Fixtures/screenshots/Home Scripts/shot.sh <window-id> docs/screenshots/<file>.png

The fixture data is written here by the CLI, against this folder — the script
refuses any other home. Nothing under a seeded home is committed.
