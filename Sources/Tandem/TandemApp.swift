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
        // One visible Tandem per machine, regardless of launch identity (terminal
        // binary vs. bundled app — see SingleInstance).
        if SingleInstance.claimOrActivate() {
            NSApp.terminate(nil)
            return
        }
        // A bare SwiftPM executable has no bundle/Info.plist, so AppKit defaults the
        // activation policy to `.prohibited`: the app can never activate and its
        // windows can never become key — the main window silently ignores all
        // keyboard input (the menu-bar item is unaffected; NSMenu tracking is
        // system-driven). Opt in to a regular app so the window can become key.
        // (A bundled launch is already `.regular`; this is what `tandem` needs.)
        NSApp.setActivationPolicy(.regular)
        NSApp.activate()
        menuBar = MenuBarController(model: TandemModel.shared)
    }

    func applicationWillTerminate(_ notification: Notification) {
        SingleInstance.release()
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
