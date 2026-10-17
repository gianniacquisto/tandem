// swift-tools-version: 6.4
import PackageDescription

// Tandem — a todo app for humans and agents on the same Mac.
//
// One package, three product targets + a test target (spec: "Package and modules"):
//   - TandemCore   (library)  — the only module with real logic: domain model,
//                               the Store (ADR 0001), and the todo operations.
//   - Tandem       (executable) — the SwiftUI GUI app (thin adapter).
//   - tandem-cli   (executable) — the agent-facing command line (thin adapter, ADR 0009).
//   - TandemCoreTests — core tests (the single test seam).
//
// The GUI and the CLI share exactly one module: TandemCore.
let package = Package(
    name: "tandem",
    platforms: [.macOS(.v27)],
    dependencies: [
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
        // Resources: the prebuilt editor bundle (ticket 07) — one HTML
        // page, one JS bundle, one CSS file + KaTeX fonts. .copy keeps the
        // directory structure intact (spec: Editor — Packaging), so the
        // Swift host loads it with
        //   Bundle.module.url(forResource: "Editor", withExtension: nil)
        // → …/Editor/index.html via WKWebView.loadFileURL, allowing read
        // access to the Editor directory (style.css, editor.js, fonts/).
        .executableTarget(
            name: "Tandem",
            dependencies: ["TandemCore"],
            resources: [.copy("Resources/Editor")],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        // The agent-facing command line (ADR 0009). Thin adapter over TandemCore:
        // it writes the store directly, under the same sidecar lock the app takes.
        .executableTarget(
            name: "tandem-cli",
            dependencies: ["TandemCore"]
        ),
        // The single test seam: the core module.
        .testTarget(
            name: "TandemCoreTests",
            dependencies: ["TandemCore"]
        )
    ]
)
