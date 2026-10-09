# 01 - Verify the official Swift MCP SDK for a stdio tools server

Type: research
Status: resolved

## Question

Can the official MCP Swift SDK (`modelcontextprotocol/swift-sdk`, ≥ 0.12) build the Tandem agent surface as intended — a minimal stdio, tools-only server in Swift — and how should the resulting executable be packaged so agent clients get a stable `command` path?

Surface, from primary sources (the SDK repo itself, plus a local build if a Swift toolchain is present):

1. The exact API surface for a tools-only stdio server: the server type, the stdio transport/session types, how a `Tool` and its JSON-Schema input are declared, and how a handler returns a text result and an error.
2. Toolchain and OS requirements (minimum Swift version, macOS floor) — confirm compatibility with a macOS 26+ deployment target.
3. Packaging: how to produce a stable-path executable from the same codebase as a SwiftUI app (SwiftPM executable target in a shared package? separate Xcode target? binary inside the app bundle at `Contents/MacOS/...`?), and the recommended install location on a personal Mac for use as an MCP `command` (e.g. a `~/bin` or `/usr/local/bin` symlink vs a direct bundle path). The path must stay stable across app rebuilds.
4. If `swift` is available on this machine: build a minimal throwaway example **outside the repo** (temp dir) — a tools-only stdio server with one trivial tool — and verify the `initialize` handshake works by piping a request in and observing the response. Capture the exact commands and observed output. If no toolchain, say so and rely on primary-source reading.

Findings land on a throwaway branch `research/swift-mcp-sdk` as a markdown file in the repo; the toy example stays out of the repo.

## Answer

Yes — the official SDK (release 0.12.1; `main` currently equals it) builds the intended surface with no gaps: `Server(name:version:capabilities:)` + `StdioTransport()` + `withMethodHandler` for `ListTools`/`CallTool`; a text result is `CallTool.Result(content: [.text(text: ..., annotations: nil, _meta: nil)], isError: false)` and errors are either `isError: true` results or thrown `MCPError`s. Toolchain floor is Swift 6.0 / macOS 13.0, so a macOS 26+ deployment target is a non-issue (verified on this machine: Swift 6.4, macOS 27.0.1 — handshake + tool round-trip both work). Packaging: a SwiftPM executable target in the same local package as the shared core module, `swift build -c release`, then copy the binary to a fixed path `~/bin/tandem-mcp` (absolute path in the MCP client config) so it stays stable across app rebuilds. Key gotchas: the README's `.text("...")` factory is deprecated (use the `.text(text:annotations:_meta:)` case), `start(transport:)` returns immediately so the executable must stay alive via `waitUntilCompleted()` (or swift-service-lifecycle), and hand-rolled `echo | server` handshake tests can lose the response if stdin closes within ~50 ms of startup — real clients keep stdin open, so it's test-only.

Full findings: branch `research/swift-mcp-sdk`, file `research/swift-mcp-sdk.md` (local repo, no remote to push to).
