import Foundation
import MCP
import TandemCore

// Tandem MCP server: a tools-only stdio server over the shared store.
//
// The three tools (`list`, `add`, `complete`) delegate straight to TandemCore,
// which owns all the semantics; this target adds only the MCP boundary — tool
// definitions, result encoding, and the error convention. See the spec's
// "MCP tool contract" for the verbatim descriptions and result shapes.

let server = Server(
    name: "tandem",
    version: "0.1.0",
    capabilities: .init(tools: .init(listChanged: false))
)

// `Server` is an actor; each registration crosses the actor boundary.
_ = await server.withMethodHandler(ListTools.self) { _ in
    ListTools.Result(tools: TandemTools.all)
}
_ = await server.withMethodHandler(CallTool.self) { params in
    await callTandemTool(params)
}

let transport = StdioTransport()
try await server.start(transport: transport)
// `start` returns once the receive loop is running; without this the process
// would exit immediately after the handshake.
await server.waitUntilCompleted()

// MARK: - Tool dispatch

/// Dispatches one `tools/call` to the core store and encodes the outcome per the
/// tool contract:
/// - success → `isError` absent, content = schema v1 JSON (+ top-level `note`
///   only when this call triggered corrupt-store recovery);
/// - domain failure → `isError: true` with short, actionable plain text
///   (an unknown `id` appends the current open list so the caller recovers in
///   one turn).
func callTandemTool(_ params: CallTool.Parameters) async -> CallTool.Result {
    let store = Store()
    do {
        switch params.name {
        case "list":
            let raw = params.arguments?["filter"]?.stringValue
            let result = try store.list(rawFilter: raw)
            let text = try StoreEncoding.listResult(result.value, note: result.note)
            return .success(text)
        case "add":
            // Missing `title` behaves as an empty title: same actionable error.
            let title = params.arguments?["title"]?.stringValue ?? ""
            let result = try store.add(title)
            let text = try StoreEncoding.todoResult(result.value, note: result.note)
            return .success(text)
        case "complete":
            // Missing `id` behaves as an unknown id: the error carries the open list.
            let id = params.arguments?["id"]?.stringValue ?? ""
            let result = try store.complete(id)
            let text = try StoreEncoding.todoResult(result.value, note: result.note)
            return .success(text)
        default:
            // A tool name we don't define is a protocol-level failure, not a
            // domain one: surface it as an error result, not a thrown MCP error.
            return CallTool.Result(
                content: [.text(text: "Unknown tool: \(params.name)", annotations: nil, _meta: nil)],
                isError: true
            )
        }
    } catch {
        return CallTool.Result(
            content: [.text(text: tandemErrorText(error), annotations: nil, _meta: nil)],
            isError: true
        )
    }
}

extension CallTool.Result {
    /// A successful tool result: a single text content block, no `isError`.
    fileprivate static func success(_ text: String) -> CallTool.Result {
        .init(content: [.text(text: text, annotations: nil, _meta: nil)])
    }
}
