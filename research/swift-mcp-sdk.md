# Research 01: Official Swift MCP SDK — stdio tools-only server verification

**Recommendation.** Use the official SDK (`modelcontextprotocol/swift-sdk`, release **0.12.1** — the current `main`): it builds Tandem's intended agent surface with no gaps. A tools-only stdio server is ~30 lines: `Server(name:version:capabilities:.init(tools: .init(listChanged: true)))` + `StdioTransport()` + two `withMethodHandler` registrations (`ListTools`, `CallTool`); a text result is `CallTool.Result(content: [.text(text: ..., annotations: nil, _meta: nil)], isError: false)`, errors are either `isError: true` results or thrown `MCPError`s. Toolchain floor is Swift 6.0 / macOS 13.0, so a macOS 26+ deployment target is a non-issue. Package the server as a **SwiftPM executable target in the same package as the shared core module**, build with `swift build -c release`, and install a **copy** at a fixed home-dir path (`~/bin/tandem-mcp`) — that path stays stable across app rebuilds. Verified end-to-end on this machine (Swift 6.4, macOS 27.0.1): the `initialize` handshake and a `tools/list`/`tools/call` round-trip both work.

- Ticket: `.scratch/tandem/issues/01-verify-swift-mcp-sdk.md`
- Date: 2026-10-09
- Machine: macOS 27.0.1 (arm64), Apple Swift 6.4 (Xcode Command Line Tools, no full Xcode installed)
- SDK revision verified: tag `0.12.1` (published 2026-05-07; `main` HEAD `a0ae212e` is the same commit, so all quotes below are from the release)

## 1. API surface for a tools-only stdio server

All types live in the library product **`MCP`** of the package `swift-sdk` (import: `import MCP`).

### Server type

`Sources/MCP/Server/Server.swift` — `public actor Server`:

```swift
public init(
    name: String,
    version: String,
    title: String? = nil,
    instructions: String? = nil,
    capabilities: Server.Capabilities = .init(),
    configuration: Configuration = .default
)

public func start(
    transport: any Transport,
    initializeHook: (@Sendable (Client.Info, Client.Capabilities) async throws -> Void)? = nil
) async throws

public func stop() async
public func waitUntilCompleted() async

@discardableResult
public func withMethodHandler<M: Method>(
    _ type: M.Type,
    handler: @escaping @Sendable (M.Parameters) async throws -> M.Result
) -> Self
```

`Server.Capabilities` is a struct of optional capability objects; the tools one is minimal:

```swift
public struct Tools: Hashable, Codable, Sendable {
    public var listChanged: Bool?
    public init(listChanged: Bool? = nil)
}
// Capabilities:
public init(completions: ..., logging: ..., prompts: ..., resources: ..., tools: Tools? = nil)
```

Key behaviors from the source:

- `start(transport:)` **returns immediately** — it registers built-in `Initialize` and `Ping` handlers, connects the transport, and spawns an internal message-loop `Task`. It does not block.
- The `initialize` handshake is built in: on `initialize` the server runs the optional `initializeHook` (lets you inspect/validate `Client.Info` + `Client.Capabilities`) and returns `Initialize.Result` with the negotiated protocol version, `capabilities`, `serverInfo`, and `instructions`.
- Version negotiation (`Sources/MCP/Base/Versioning.swift`): supported protocol versions are `["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"]`; it echoes the client's version if supported, otherwise its latest.
- Handler errors are caught and converted to JSON-RPC errors: `error as? MCPError ?? MCPError.internalError(...)`.
- `configuration: .strict` (default `.default` = non-strict) can require initialize-before-everything; default is lenient.

### Stdio transport

`Sources/MCP/Base/Transports/StdioTransport.swift` — `public actor StdioTransport: Transport`. Newline-delimited JSON-RPC over stdin/stdout (the MCP stdio transport); `FileDescriptor` comes from Apple's `swift-system` package.

```swift
public init(
    input: FileDescriptor = FileDescriptor.standardInput,
    output: FileDescriptor = FileDescriptor.standardOutput,
    logger: Logger? = nil
)
public func connect() async throws
public func disconnect() async
public func send(_ message: Data) async throws
public func receive() -> AsyncThrowingStream<Data, Swift.Error>
```

The read loop uses non-blocking reads with a 10 ms EAGAIN backoff, and finishes the stream on EOF (so a client closing stdin shuts the server down — verified in the smoke test).

### Tool declaration + JSON-Schema input

`Sources/MCP/Server/Tools.swift` — `public struct Tool: Hashable, Codable, Sendable`:

```swift
public init(
    name: String,
    title: String? = nil,
    description: String?,
    inputSchema: Value,
    annotations: Annotations = nil,
    outputSchema: Value? = nil,
    icons: [Icon]? = nil,
    _meta: Metadata? = nil
)
```

The input schema is a **`Value`** (`Sources/MCP/Base/Value.swift`) — a free-form JSON value enum (`.object([String: Value])`, `.array([Value])`, `.string`, `.int`, `.bool`, `.null`, ...) that is `ExpressibleByDictionaryLiteral`/`ArrayLiteral`/`StringLiteral`, so you write the JSON Schema inline:

```swift
Tool(
    name: "add",
    description: "Add a new todo",
    inputSchema: .object([
        "type": "object",
        "properties": .object([
            "title": .object(["type": "string", "description": "Todo title"])
        ]),
        "required": .array(["title"])
    ])
)
```

There is no schema-validation helper — the schema is authored by hand (fine for the v1 surface: `list` / `add` / `complete`).

### Method handlers, text results, and errors

`ListTools` and `CallTool` are `Method` enums in the same file:

```swift
public enum ListTools: Method {
    public static let name = "tools/list"
    public struct Result: Hashable, Codable, Sendable {
        public let tools: [Tool]
        public let nextCursor: String?
        public init(tools: [Tool], nextCursor: String? = nil, _meta: Metadata? = nil)
    }
}

public enum CallTool: Method {
    public static let name = "tools/call"
    public struct Parameters: Hashable, Codable, Sendable {
        public let _meta: Metadata?
        public let name: String
        public let arguments: [String: Value]?
        public init(name: String, arguments: [String: Value]? = nil, meta: Metadata? = nil)
    }
    public struct Result: Hashable, Codable, Sendable {
        public let content: [Tool.Content]
        public let structuredContent: Value?
        public let isError: Bool?
        public init(content: [Tool.Content] = [], structuredContent: Value? = nil,
                    isError: Bool? = nil, _meta: Metadata? = nil)
    }
}
```

`Tool.Content` text case (current form in 0.12.1):

```swift
public enum Content: Hashable, Codable, Sendable {
    case text(text: String, annotations: Resource.Annotations?, _meta: Metadata?)
    // + .image, .audio, .resource, .resourceLink
}
```

- **Text result**: `return .init(content: [.text(text: "...", annotations: nil, _meta: nil)], isError: false)`
- **Tool-level (soft) error** — the MCP-spec way for tool failures: `return .init(content: [.text(text: "Missing title parameter", annotations: nil, _meta: nil)], isError: true)`
- **Protocol-level error** — throw from the handler; `Server` encodes it as a JSON-RPC error response. `MCPError` (`Sources/MCP/Base/Error.swift`):

```swift
public enum MCPError: Swift.Error, Sendable {
    case parseError(String?)        // -32700
    case invalidRequest(String?)    // -32600
    case methodNotFound(String?)    // -32601
    case invalidParams(String?)     // -32602
    case internalError(String?)     // -32603
    case serverError(code: Int, message: String)
    case urlElicitationRequired(message: String, elicitations: [URLElicitationInfo])  // -32042
    case connectionClosed
    case transportError(Swift.Error)
}
```

- Arguments arrive as `[String: Value]?`; read them with `params.arguments?["title"]?.stringValue` (`Value` has `stringValue`/`intValue`/`boolValue`/`doubleValue` accessors).
- Convenience for structured output: `CallTool.Result` also has a `structuredContent: Value?` field plus a throwing `init<Output: Codable>(content:structuredContent:isError:)`.

### Minimal working server (this is the shape of Tandem's)

```swift
import MCP

let server = Server(
    name: "tandem",
    version: "0.1.0",
    capabilities: .init(tools: .init(listChanged: false))
)

await server.withMethodHandler(ListTools.self) { _ in .init(tools: [/* list, add, complete */]) }

await server.withMethodHandler(CallTool.self) { params in
    switch params.name {
    case "list":
        return .init(content: [.text(text: store.listText(), annotations: nil, _meta: nil)], isError: false)
    default:
        return .init(content: [.text(text: "Unknown tool: \(params.name)", annotations: nil, _meta: nil)], isError: true)
    }
}

let transport = StdioTransport()
try await server.start(transport: transport)
await server.waitUntilCompleted()   // blocks until stdin EOF
```

The README's "Graceful Shutdown" section recommends wrapping `start`/`stop` in [swift-service-lifecycle](https://github.com/swift-server/swift-service-lifecycle) (`Service` protocol) for SIGINT/SIGTERM handling in a long-lived server. For Tandem (spawned per agent session, dies when the client closes stdin) `waitUntilCompleted()` is sufficient; ServiceLifecycle is an optional hardening.

## 2. Toolchain and OS requirements

| Requirement | Value | Source |
|---|---|---|
| Swift | **6.0+** (README); the `0.12.1` tag ships both `Package.swift` (`swift-tools-version:6.1`) and a `Package@swift-6.0.swift` fallback (Swift 6.0 toolchains get the slim manifest with just the `MCP` library) | README "Requirements"; both manifests at tag `0.12.1` |
| macOS floor | **13.0** (both manifests: `platforms: [.macOS("13.0"), ...]`) | `Package.swift` @ `0.12.1` |
| Xcode | README says "Xcode 16+"; in practice only the Swift 6 toolchain is needed — the smoke test built with Command Line Tools (no full Xcode) | README; local build |
| Linux | glibc/musl distributions (not needed for Tandem) | README "Platform Availability" |
| SDK deps | `swift-system`, `swift-log`, `mattt/eventsource` (Apple platforms); `swift-nio` only for the conformance-test executables | `Package.swift` @ `0.12.1` |
| Spec version implemented | MCP **2025-11-25** (latest); negotiates down to 2024-11-05 | README; `Versioning.swift` |

**macOS 26+ deployment target: fully compatible** — 13.0 floor is far below 26, and the server is a plain executable (no app-bundle APIs involved). Verified on this machine: Swift 6.4 / macOS 27.0.1 builds and runs (see §4).

## 3. Packaging the server executable

Three candidate shapes, evaluated:

1. **SwiftPM executable target in a shared local package** — *recommended.* Structure: one local package with (a) a `TandemCore` library target (todo model + `~/.tandem/todos.json` store logic) and (b) an `executableTarget(name: "tandem-mcp-server", dependencies: ["TandemCore", .product(name: "MCP", package: "swift-sdk")])`. The Xcode SwiftUI app target references the same local package for `TandemCore`. `swift build -c release` produces a single self-contained executable at `.build/release/tandem-mcp-server` (SDK links only Foundation + swift-log + swift-system — all system/static; nothing to distribute alongside). One codebase, one build system for the server, and the app and server share the core module exactly as the map's destination describes.
2. **Separate Xcode command-line tool target** — works, but Xcode's DerivedData output location is volatile and you'd copy the binary to a stable path anyway; you'd maintain two build systems for shared code. No advantage over (1) for a personal tool.
3. **Binary inside the app bundle at `Tandem.app/Contents/MacOS/tandem-mcp`** — *not recommended.* The path only stays stable if the app always lives at the same absolute location; replacing the bundle (app updates), moving the app, or having two versions installed all break it. Auxiliary executables inside a signed app must be signed too (Apple Silicon enforces signing on all executable code in a bundle), adding a signing step for zero benefit here.

### Stable-path install (personal Mac)

- **Install location: a copy at `~/bin/tandem-mcp`** (i.e. `/Users/gianniacquisto/bin/tandem-mcp`). Rationale:
  - A **copy** (not a symlink into `.build/`) survives `swift package clean` / re-resolves / DerivedData wipes; the path never changes across rebuilds, which is the hard constraint.
  - `~/bin` needs no sudo (vs `/usr/local/bin`), is per-user — matching the "local agents, same OS user, same Mac" constraint — and is the conventional place for personal tools. `/usr/local/bin/tandem-mcp` is an equivalent choice if you prefer it; either way the MCP config stores the **absolute path**, so PATH membership doesn't even matter to the client.
  - Reinstall flow (postbuild script or one-liner): `swift build -c release && cp .build/release/tandem-mcp-server ~/bin/tandem-mcp`. Overwriting the file while a client has it running is safe on macOS (the running process keeps the old inode; new spawns get the new binary).
  - MCP client config then reads e.g. `"command": "/Users/gianniacquisto/bin/tandem-mcp"` (exact `opencode.json` / `.mcp.json` snippets are deferred to the agent-setup docs, per the map's "Not yet specified").
- Signing note: `swift build` ad-hoc-signs the binary, which is all Apple Silicon requires for direct `exec` by the agent clients. No notarization involved (distribution is explicitly out of scope).

## 4. Smoke test (built and run on this machine)

Toolchain: `Apple Swift version 6.4 (swiftlang-6.4.0.34.1 clang-2100.3.34.1)`, `macOS 27.0.1`, arm64, Command Line Tools only. Package created **outside the repo** at `/private/var/folders/4x/kz_77l9s0lddszts3p0k6b940000gn/T/opencode/tandem-mcp-smoke` (throwaway; not committed).

### Package.swift

```swift
// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "tandem-mcp-smoke",
    platforms: [.macOS("13.0")],
    dependencies: [
        .package(url: "https://github.com/modelcontextprotocol/swift-sdk.git", from: "0.12.0")
    ],
    targets: [
        .executableTarget(
            name: "smoke-server",
            dependencies: [.product(name: "MCP", package: "swift-sdk")]
        )
    ]
)
```

### Sources/smoke-server/main.swift (tools-only stdio server, one trivial `echo` tool)

```swift
import MCP

let server = Server(
    name: "tandem-smoke",
    version: "0.1.0",
    capabilities: .init(tools: .init(listChanged: false))
)

await server.withMethodHandler(ListTools.self) { _ in
    .init(tools: [
        Tool(
            name: "echo",
            description: "Echo back the provided text",
            inputSchema: .object([
                "type": "object",
                "properties": .object([
                    "text": .object(["type": "string", "description": "Text to echo"])
                ]),
                "required": .array(["text"])
            ])
        )
    ])
}

await server.withMethodHandler(CallTool.self) { params in
    guard params.name == "echo" else {
        return .init(content: [.text(text: "Unknown tool: \(params.name)", annotations: nil, _meta: nil)],
                     isError: true)
    }
    let text = params.arguments?["text"]?.stringValue ?? ""
    return .init(content: [.text(text: "echo: \(text)", annotations: nil, _meta: nil)], isError: false)
}

let transport = StdioTransport()
try await server.start(transport: transport)
await server.waitUntilCompleted()
```

### Build

```console
$ swift build -c release
...
Build complete! (35,63 secs)
```

Resolved dependency (`Package.resolved`): `swift-sdk` **0.12.1**, revision `a0ae212ebf6eab5f754c3129608bc5557637e605`. (Two harmless linker warnings about missing CommandLineTools search paths.)

### Handshake + round-trip

A Python client (`client_test.py` in the same temp dir) spawns the binary, writes `initialize` → `notifications/initialized` → `tools/list` → `tools/call` (newline-delimited JSON-RPC, stdin left open while reading — same behavior as a real MCP client), then closes stdin:

```console
$ python3 client_test.py
{"id":1,"jsonrpc":"2.0","result":{"capabilities":{"tools":{"listChanged":false}},"protocolVersion":"2025-06-18","serverInfo":{"name":"tandem-smoke","version":"0.1.0"}}}
{"id":3,"jsonrpc":"2.0","result":{"content":[{"text":"echo: hello tandem","type":"text"}],"isError":false}}
{"id":2,"jsonrpc":"2.0","result":{"tools":[{"description":"Echo back the provided text","inputSchema":{"properties":{"text":{"description":"Text to echo","type":"string"}},"required":["text"],"type":"object"},"name":"echo"}]}}
exit: 0
```

Observations:

- **`initialize` handshake works**: response returns the negotiated `protocolVersion` (echoed `2025-06-18`), the advertised `tools` capability, and `serverInfo`.
- `tools/list` returns the tool with the exact JSON Schema as authored; `tools/call` returns the text result with `isError:false`.
- Responses arrived out of order (id 3 before id 2) — handlers run in per-request tasks; JSON-RPC does not require ordering. Not a bug.
- After stdin EOF the process exited cleanly with code 0 (the transport finishes its stream on EOF, `waitUntilCompleted()` returns).

### Gotcha found: do not close stdin immediately after writing

```console
$ (printf '<initialize request>\n'; sleep 0)   | ./.build/release/smoke-server   # 0 bytes out
$ (printf '<initialize request>\n'; sleep 0.05) | ./.build/release/smoke-server   # 169 bytes (the response)
```

If the pipe's write end closes within ~50 ms of process start (i.e. EOF is already pending when the transport's read loop starts), the server exits **without emitting the response** — the read loop yields the buffered message(s) into the transport's `AsyncThrowingStream` and calls `finish()` before the server's message-loop consumer starts iterating, and the buffered element is lost on this runtime (Swift 6.4). Real MCP clients (OpenCode, Claude Code) keep stdin open for the session, so this is a **test-only** trap: `echo '...' | server` will look broken; use a client that holds stdin open (as `client_test.py` does). No action needed for the production design; noted so the smoke test in the spec doesn't mislead.

## Gotchas checklist (for the spec/implementation)

1. `.text("...")` / `.text(text: "...")` static factories on `Tool.Content` are **deprecated** in 0.12.1 — use the enum case form `.text(text: ..., annotations: nil, _meta: nil)`. (The README examples still show the deprecated form.)
2. `server.start(transport:)` returns immediately; a bare executable must keep running — use `await server.waitUntilCompleted()` (or swift-service-lifecycle for signal handling).
3. Keep stdin open in any hand-rolled handshake test (see §4 gotcha).
4. Advertise tools via `capabilities: .init(tools: .init(listChanged: true|false))` at `Server` init; it's serialized into the `initialize` response.
5. `inputSchema` is a free-form `Value` — no validation helper; author the JSON Schema carefully (agents rely on it to call tools correctly).
6. Pin the SDK to a release (`from: "0.12.0"` resolved to 0.12.1). `main` currently equals 0.12.1, but don't depend on that.
7. `arguments` is `[String: Value]?` — always optional; use `?.stringValue` accessors.

## Sources

- `modelcontextprotocol/swift-sdk` @ tag `0.12.1` (= `main` HEAD `a0ae212e`), fetched 2026-10-09:
  - `README.md` (requirements, installation, server usage, transports, platform availability)
  - `Package.swift` and `Package@swift-6.0.swift`
  - `Sources/MCP/Server/Server.swift` (Server, Capabilities, start/stop/waitUntilCompleted, withMethodHandler, handler-error conversion)
  - `Sources/MCP/Server/Tools.swift` (Tool, Tool.Content, ListTools, CallTool)
  - `Sources/MCP/Base/Transports/StdioTransport.swift`
  - `Sources/MCP/Base/Value.swift`
  - `Sources/MCP/Base/Error.swift` (MCPError)
  - `Sources/MCP/Base/Versioning.swift` (supported protocol versions)
- GitHub releases API: latest release `0.12.1` (published 2026-05-07).
- MCP spec (context only; SDK source was sufficient): modelcontextprotocol.io, spec version 2025-11-25, stdio transport + tools.
- Local build and smoke test on this machine (commands and output in §4).
