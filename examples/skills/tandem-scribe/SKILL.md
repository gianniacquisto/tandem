---
name: tandem-scribe
description: "Dispatch the scribe to keep what a session turned up. Reach for it when a decision settles, something surprising turns up, or a refine, sprint or interview pass ends."
---

# Tandem scribe

The scribe (`agents/scribe.md`) judges what the work turned up and writes it to the notes, or proposes a docs change, or notes nothing. The bar for what is worth keeping lives in that file. You do not judge or write the note yourself.

1. **When**: at the end of each pass, one scribe per ticket the pass carried, dispatched when that ticket's part of the pass ends: refine, after its refiner reports; sprint and drain, when the ticket reaches its exit state; interview, after its decision is recorded. Outside a pass: when a decision settles or something surprising turns up.
2. **How**: on `sonnet`, low effort, never the session's own model. Fork the session's context where the host can. Otherwise hand over what happened (a few lines: for a sprint ticket, the exit state, the PR number and any gate finding quoted), the ticket ref, and the files or pages involved. Dispatch in the background where the host allows it.
3. **The report**: the scribe's `SCRIBE:` line goes back to whoever dispatched it, and the dispatcher relays it in its own final report: the sprint and drain close report carries a `Notes:` block with one `SCRIBE:` line per batch ticket, the refine pass relays it beside the refiner's line, the interview adds it to its closing table, and an ad hoc session says it in its reply. A dispatcher waits for outstanding scribes before its final report. Nothing else carries it: a note write gets no ticket comment, because the note is the durable record.
4. **No dispatch possible**: a host with no subagents has the session apply `agents/scribe.md` itself, so the Memory tier still works alone (README.md: each tier works without the next).
