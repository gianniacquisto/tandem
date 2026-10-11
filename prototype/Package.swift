// swift-tools-version: 6.2
import PackageDescription

// PROTOTYPE — throwaway, not the real app.
// Answers ticket 01 (v2 map): "How far does the Ligne Claire theme reach?"
// Three reach variants (A brand-only / B subtle / C deeper) of both faces —
// main window + themed menu bar popover — in light and dark, plus draft
// assets (tandem-bike illustration, wordmark treatments, candidate palette).
// Run: cd prototype && swift run TandemPrototype
// Export draft-asset PNGs: swift run TandemPrototype --export-previews
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
