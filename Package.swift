// swift-tools-version: 6.4
import PackageDescription

// Tandem — a todo app for humans and agents on the same Mac.
//
// One package, three product targets + a test target (spec: "Package and modules"):
//   - TandemCore   (library)  — the only module with real logic: domain model,
//                               the Store (ADR 0001), and the todo operations.
//   - Tandem       (executable) — the SwiftUI GUI app (thin adapter).
//   - tandem-mcp   (executable) — the MCP stdio server (thin adapter).
//   - TandemCoreTests — core tests (the single test seam).
//
// The GUI and the MCP server share exactly one module: TandemCore.
let package = Package(
    name: "tandem",
    platforms: [.macOS(.v27)],
    dependencies: [
        // Official MCP Swift SDK (ticket 01: release 0.12.1; floor Swift 6.0 / macOS 13.0).
        .package(url: "https://github.com/modelcontextprotocol/swift-sdk.git", from: "0.12.0"),
        // swift-markdown (Apache-2.0, cmark-gfm-backed, thread-safe persistent AST):
        // the in-process vault index — wikilink scanning, backlinks, search (spec:
        // "Package and modules"). Same GFM dialect the CM6 editor highlights.
        // NOTICE.txt stays in the repo root (spec: "Its NOTICE stays with the app").
        .package(url: "https://github.com/apple/swift-markdown.git", from: "0.9.0")
    ],
    targets: [
        // The core module: domain model + Store + operations. Concurrency-clean (Swift 6).
        .target(
            name: "TandemCore",
            dependencies: [
                .product(name: "Markdown", package: "swift-markdown")
            ]
        ),
        // The GUI app. AppKit/SwiftUI interop stays in Swift 5 language mode to avoid
        // strict-concurrency friction; all logic lives in TandemCore.
        .executableTarget(
            name: "Tandem",
            dependencies: ["TandemCore"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        // The MCP stdio server. Thin adapter over the official Swift SDK + TandemCore.
        .executableTarget(
            name: "tandem-mcp",
            dependencies: [
                "TandemCore",
                .product(name: "MCP", package: "swift-sdk")
            ]
        ),
        // The single test seam: the core module.
        .testTarget(
            name: "TandemCoreTests",
            dependencies: ["TandemCore"]
        )
    ]
)
