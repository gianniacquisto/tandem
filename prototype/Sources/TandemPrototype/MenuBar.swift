import SwiftUI
import AppKit
import Observation

// PROTOTYPE — real NSStatusItem + themed NSPopover, so the popover face can
// be judged where it will live. Left-click toggles the popover; right-click
// gives utility items (Open Tandem / Quit) — those are not design decisions.

@MainActor
final class MenuBarController: NSObject {
    private let model: TandemModel
    private let paletteProvider: () -> Palette
    private let statusItem: NSStatusItem
    private let popover = NSPopover()
    private var appearanceMode: () -> AppearanceMode

    init(model: TandemModel,
         paletteProvider: @escaping () -> Palette,
         appearanceMode: @escaping () -> AppearanceMode) {
        self.model = model
        self.paletteProvider = paletteProvider
        self.appearanceMode = appearanceMode

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()

        if let button = statusItem.button {
            button.image = Self.templateGlyph()
            button.action = #selector(click)
            button.sendAction(on: [.leftMouseUp, .rightMouseUp])
            button.toolTip = "Tandem (prototype)"
        }

        popover.behavior = .transient
        popover.animates = true
    }

    /// Template image of the bare bike (system tints it per context).
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
        switch appearanceMode() {
        case .dark:  popover.appearance = NSAppearance(named: .darkAqua)
        case .light: popover.appearance = NSAppearance(named: .aqua)
        case .system: popover.appearance = nil
        }
        popover.contentViewController = NSHostingController(
            rootView: PopoverFaceView(
                model: model,
                palette: paletteProvider(),
                onOpenApp: activateApp
            )
        )
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func showUtilityMenu() {
        let menu = NSMenu()
        menu.addItem(withTitle: "Open Tandem", action: #selector(activateApp), keyEquivalent: "")
        menu.addItem(.separator())
        menu.addItem(withTitle: "Quit Tandem (prototype)", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        statusItem.menu = menu
        statusItem.button?.performClick(nil)
        statusItem.menu = nil
    }

    @objc private func activateApp() {
        popover.performClose(nil)
        NSApp.activate(ignoringOtherApps: true)
        for window in NSApp.windows where window.title.hasPrefix("Tandem") {
            window.makeKeyAndOrderFront(nil)
            break
        }
    }
}
