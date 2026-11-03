Noted 2026-11-02 against `opencode` v2.2.5
(`sha256 abe4d88594bffb12b364375b8a6732d2d1f7a977195ed1a35c5c76f8e89c9e0e`,
arm64 macOS), the pinned binary `docs/adr/0038-the-harness-home.md` names.

`session.json` is `GET /api/session/ses_f26dab226ffed3tYjvxcXUQwEg`
verbatim, and `messages-page-1.json` is
`GET /api/session/ses_f26dab226ffed3tYjvxcXUQwEg/message?limit=100`
verbatim, both read over the loopback API with
`Authorization: Basic base64("opencode:" + password)` from
`~/.local/state/opencode/service.json`. The session spent
tokens 25765/1309/2743, cache read 78032, cost 0, model
`space-bunny-free`, and loaded the `opencode` skill once. The page's
`cursor.next` names an empty second page, so the reader follows it
and stops.

Edge cases beside this file are hand-written and minimal: an empty
message list, a null model, a 401 and a 404. They pin the error
mapping, not the harness.
