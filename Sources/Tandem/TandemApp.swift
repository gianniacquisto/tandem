import SwiftUI
import AppKit

@main
struct TandemApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        // Main window — a regular app (Dock icon, standard window, spec user story 20).
        Window("Tandem", id: "main") {
            MainView()
                .environment(TandemModel.shared)
        }
        .windowResizability(.contentMinSize)
        .defaultSize(width: 470, height: 600)
    }
}

/// Minimal app delegate. It owns the AppKit menu-bar item (see `MenuBar`) and keeps
/// the app resident in the menu bar after the main window is closed. SwiftUI provides
/// the standard main menu (App / Edit / Quit, …) automatically.
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var menuBar: MenuBarController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        menuBar = MenuBarController(model: TandemModel.shared)
    }

    // Stay alive in the menu bar when the main window is closed.
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}

/// Bridges the AppKit menu bar's "Open Tandem" to SwiftUI's window management. SwiftUI
/// sets `openMain` (it holds the `openWindow` action); the menu bar calls it.
final class WindowManager {
    static let shared = WindowManager()
    private init() {}
    var openMain: (() -> Void)?
}
