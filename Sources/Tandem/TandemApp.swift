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
                .environment(BikeshedModel.shared)
                .environment(SetupPresenter.shared)
        }
        .windowResizability(.contentMinSize)
        .defaultSize(width: 1060, height: 680)
        .commands {
            // The human picks the bikeshed from the app; the choice is
            // persisted and applied live — no restart.
            CommandGroup(after: .newItem) {
                Button("Choose Bikeshed…") {
                    chooseBikeshed()
                }
                .keyboardShortcut("v", modifiers: [.command, .shift])
                // The first-run setup surface, re-openable later.
                Button("Setup…") {
                    SetupPresenter.shared.isPresented = true
                }
            }
        }
    }
}

/// Runs the "Choose Bikeshed…" folder picker and hands the result to the
/// model, which switches the bikeshed live. A folders-only, single-selection
/// `NSOpenPanel`; the chosen folder becomes the bikeshed (a new folder may
/// be created). The one picker path: the File menu command and the setup
/// wizard both call this.
func chooseBikeshed() {
    let panel = NSOpenPanel()
    panel.canChooseDirectories = true
    panel.canChooseFiles = false
    panel.allowsMultipleSelection = false
    panel.canCreateDirectories = true
    panel.prompt = "Choose"
    panel.message = "Choose the folder Tandem uses as your bikeshed. "
        + "Its markdown files become your notes."
    if panel.runModal() == .OK, let url = panel.url {
        BikeshedModel.shared.switchBikeshed(to: url)
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
