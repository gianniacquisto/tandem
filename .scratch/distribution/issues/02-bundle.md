# 02: Ship the MCP binary inside the app bundle (U2)

**What to build:** The bundle script takes the MCP binary as a second input and assembles `Tandem.app` with it at the stable in-bundle path `Contents/MacOS/tandem-mcp` — the path agent configs will reference. The resulting bundle is ad-hoc signed, verifies, and the in-bundle MCP binary passes the stdin-EOF smoke (the same smoke the old formula's `brew test` used).

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

Context: [spec](../spec.md) — "Release pipeline" (step 3), "Code surface", and the U2 unit brief under "Execution model". ADR 0003 ("Why the MCP binary lives in the app bundle").

- [ ] `Scripts/make-app-bundle.sh` accepts the MCP binary as an input and places it at `Tandem.app/Contents/MacOS/tandem-mcp`
- [ ] The existing whole-bundle ad-hoc `codesign` covers the MCP binary; `codesign --verify --strict` passes on the result
- [ ] The in-bundle binary passes the stdin-EOF smoke: launch with stdin/stdout/stderr to `/dev/null`, clean exit
- [ ] The rest of the bundle tree is unchanged (versioned Info.plist, icon, brand photo)
- [ ] Evidence captured: the resulting bundle tree, `codesign --verify` output, and the smoke exit code
