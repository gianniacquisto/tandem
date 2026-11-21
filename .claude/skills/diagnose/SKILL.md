---
name: diagnose
description: "Reproduce and diagnose a Tandem bug or UX issue on a scratch server from this checkout, with the server log, the browser console and the network in view. Reach for it when asked to debug, investigate or reproduce a board bug, check how a screen looks or behaves, or audit the board's UX."
---

# Diagnose

The loop runs on a **scratch** server: this checkout's build, serving a throwaway copy of a committed fixture home on `127.0.0.1:7790`. The human's own `tandem serve` (port 7700, `~/.tandem`) is their real board; the diagnosis reads and writes only the scratch one, and files its finding on the real one.

## 1. Start the scratch server

`preview_start` with `name: "tandem-scratch"` (`.claude/launch.json`). It runs `Scripts/scratch-serve.sh`, which builds `tandem`, copies `Fixtures/port/Board` (78 tickets, every status, labels, blocked edges) into a fresh home and serves it with `TANDEM_LOG=info,tandem_core=debug`. For empty states use `tandem-scratch-empty` (port 7791, no tickets).

Done when the board renders in the browser pane and `preview_logs` shows the `scratch-serve: home <path>` line. Keep that path: it is the home every later step inspects.

## 2. Reproduce until red

Drive the board in the browser pane (`find`, `computer`, `form_input`) along the path the report describes. The loop is **red** when the failure is an observable you can quote:

- a screenshot or `read_page` text showing the wrong state;
- an error in `read_console_messages` (`tandem.js` logs every failed card move and note save there);
- a failed or surprising call in `read_network_requests` (htmx swaps, `GET /events`, `POST /mcp`);
- a `WARN` or `ERROR` line in `preview_logs` (`level: "error"`, or `search` for the route).

An agent-side bug reproduces through the scratch server's `/mcp`: run the CLI against it, `TANDEM_SERVER=http://127.0.0.1:7790 TANDEM_AGENT=builder rust/target/debug/tandem <verb> ...`, so the actor is recorded the way an agent's write would be. The plugin's MCP tools reach the human's real server, never the scratch one.

A report that will not go red after three honest attempts is a finding too: say what you tried and what you saw.

## 3. Gather the evidence

Collect every source, not only the first that shows something:

- **Server**: `preview_logs`, around the moment of the failure.
- **Client**: `read_console_messages` with `onlyErrors`, and `read_network_requests` for the status and body of the failing call.
- **Store**: `TANDEM_HOME=<scratch home> rust/target/debug/tandem doctor` for integrity, references and schema version, and `TANDEM_HOME=<scratch home> rust/target/debug/tandem ticket show <id>` for one ticket's stored state. The plugin's `doctor_report` tool reads the real home, not this one.

## 4. For a UX or visual issue, look in every frame

- `resize_window` with `preset: "mobile"`, then `"desktop"`; reload after each switch.
- `resize_window` with `colorScheme: "dark"` and `"light"`.
- The style picker in the header switches palettes (`neutral`, `tandem`, and the rest in `rust/core/src/serve/static/theme.js`); check `neutral` and `tandem` at least.
- Judge with the `ux` and `ui-design` lenses (`examples/skills/review-lenses/ux.md`, `ui-design.md`) against `DESIGN.md`; `rust/core/src/serve/static/tokens.css` wins on any value.
- For contrast, focus order or keyboard reach, run the `design:accessibility-review` skill on the screen.

Reset the viewport with `preset: "desktop"` when done.

## 5. Find the cause

Trace the red observable to code: templates in `rust/core/templates/`, client behaviour in `rust/core/src/serve/static/tandem.js`, routes in `rust/core/src/serve/`, error wording in `rust/core/src/domain/error.rs`. The cause is found when you can name the file and line and say why it produces exactly the observable from step 2.

## 6. Record and stop

- File the finding on the human's board with `ticket_add`: the steps to red, the quoted evidence from step 3, the cause as `path:line`, and the fixture used. A fix the human asked for runs test-first through the `tdd` skill.
- `preview_stop` the scratch server. Its home is in the temp folder and is not cleaned by hand.
