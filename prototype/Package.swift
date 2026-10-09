// swift-tools-version: 6.2
import PackageDescription

// PROTOTYPE — throwaway, not the real app.
// Answers ticket 04: "What should Tandem look like?"
// Three structurally different variants of the main window + a live menu bar
// item stub, cycled from a floating bottom bar. Run: swift run TandemPrototype
let package = Package(
    name: "tandem-prototype",
    platforms: [.macOS(.v26)],
    targets: [
        .executableTarget(
            name: "TandemPrototype",
            path: "Sources/TandemPrototype",
            swiftSettings: [.swiftLanguageMode(.v5)]
        )
    ]
)
