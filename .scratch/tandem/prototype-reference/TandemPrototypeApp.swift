import SwiftUI
import AppKit

// PROTOTYPE — throwaway, not the real app.
// Run: cd prototype && swift run TandemPrototype
//
// "Three variants of the Tandem main window + a live menu bar item stub,
// cycled from the floating bottom bar (or ←/→). See README.md."

@main
struct TandemPrototypeApp: App {
    private let model = TandemModel()
    private let icon = IconState()
    private let menuController: MenuBarController
    @State private var variant: WindowVariant = .a

    init() {
        menuController = MenuBarController(model: model, icon: icon)
    }

    var body: some Scene {
        WindowGroup("Tandem — PROTOTYPE") {
            RootView(model: model, icon: icon, variant: $variant)
        }
    }
}
