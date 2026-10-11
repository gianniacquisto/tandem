import AppKit
import SwiftUI
import TandemCore

/// The menu bar item (spec user stories 12–19) — a real `NSStatusItem` opening
/// a themed `NSPopover` with SwiftUI content, replacing v1's AppKit `NSMenu`.
/// The menu bar is the most-visible brand surface, so it wears the theme
/// (ticket 01: the themed popover was kept with the reach B verdict).
///
/// Left-click toggles the popover; right-click gives utility items
/// (Open Tandem / Quit). The popover follows the system appearance — the app
/// never forces one (map).
///
/// Popover, top to bottom: bare-bike mark + wordmark header, quick-add field
/// (Return commits), up to 5 open todos (the checkbox completes — one-way; a
/// plain row click is never destructive), "… n more" / "All clear", "Open
/// Tandem". The content observes the shared model, so store changes (e.g. an
/// agent completing an item) refresh it live.

@MainActor
final class MenuBarController: NSObject {
    private let model: TandemModel
    private let statusItem: NSStatusItem
    private let popover = NSPopover()

    init(model: TandemModel) {
        self.model = model
        self.statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()

        if let button = statusItem.button {
            button.image = Self.templateGlyph()
            button.action = #selector(click)
            button.sendAction(on: [.leftMouseUp, .rightMouseUp])
            button.toolTip = "Tandem"
        }

        popover.behavior = .transient
        popover.animates = true
        // No deinit cleanup: the controller is app-lifetime (owned by the app
        // delegate), and the process exit reclaims the status item.
    }

    /// Template image of the bare bike (the double saddle is the tandem tell) —
    /// the system tints it per context; plain, no count badge.
    private static func templateGlyph() -> NSImage {
        let pal = Palette(
            paper: .clear, parchment: .clear, sand: .clear, shadow: .clear,
            ink: .black, inkDim: .black, dustyBlue: .black, dustyBlueDeep: .black,
            ember: .black, emberDeep: .black, moss: .black, mossDeep: .black,
            sky: .clear, hill: .clear, hillFar: .clear, skin: .black, sun: .black
        )
        let size = CGSize(width: 27, height: 18)
        let renderer = ImageRenderer(content:
            BikeScene(palette: pal, riders: false, lineWidth: 6)
                .frame(width: size.width, height: size.height)
        )
        renderer.scale = 2
        let image = renderer.nsImage ?? NSImage(size: size)
        image.size = size
        image.isTemplate = true
        return image
    }

    // MARK: - Clicks

    @objc private func click() {
        if NSApp.currentEvent?.type == .rightMouseUp {
            showUtilityMenu()
        } else {
            togglePopover()
        }
    }

    private func togglePopover() {
        guard let button = statusItem.button else { return }
        if popover.isShown {
            popover.performClose(nil)
            return
        }
        // Follow the system appearance; the popover content picks its palette
        // from this effective appearance (day panel / night panel).
        let scheme = NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) ?? .aqua
        popover.appearance = NSAppearance(named: scheme)
        popover.contentViewController = NSHostingController(
            rootView: PopoverFaceView(model: model, onOpenApp: { [weak self] in
                self?.openApp()
            })
        )
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func showUtilityMenu() {
        let menu = NSMenu()
        let open = NSMenuItem(title: "Open Tandem", action: #selector(openApp), keyEquivalent: "")
        open.target = self
        menu.addItem(open)
        menu.addItem(.separator())
        menu.addItem(withTitle: "Quit Tandem", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        statusItem.menu = menu
        statusItem.button?.performClick(nil)
        statusItem.menu = nil
    }

    // MARK: - Actions

    @objc private func openApp() {
        popover.performClose(nil)
        NSApp.activate(ignoringOtherApps: true)
        WindowManager.shared.openMain?()
    }
}
