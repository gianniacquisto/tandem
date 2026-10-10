import AppKit

// PROTOTYPE — throwaway. A real NSStatusItem so the menu bar stub can be
// judged in situ, in the real menu bar. The glyph/badge are cycled from the
// floating bar — surfaced, not picked (ticket 04 keeps that decision open).

final class MenuBarController: NSObject, NSMenuDelegate {
    private let model: TandemModel
    private let icon: IconState
    private let statusItem: NSStatusItem
    private let menu = NSMenu()
    private var observer: NSObjectProtocol?

    init(model: TandemModel, icon: IconState) {
        self.model = model
        self.icon = icon
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()

        menu.autoenablesItems = false
        menu.delegate = self
        statusItem.menu = menu
        statusItem.button?.toolTip = "Tandem (prototype)"
        updateImage()

        observer = NotificationCenter.default.addObserver(
            forName: .tandemChanged, object: nil, queue: .main
        ) { [weak self] _ in
            self?.updateImage()
        }
    }

    deinit {
        if let observer { NotificationCenter.default.removeObserver(observer) }
        NSStatusBar.system.removeStatusItem(statusItem)
    }

    private func updateImage() {
        statusItem.button?.image = Self.statusImage(
            glyph: icon.glyph.symbol,
            badged: icon.badged,
            openCount: model.openTodos.count
        )
    }

    // MARK: - Menu

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()

        // Quick-add field at the top; Return commits.
        let add = NSMenuItem()
        add.view = QuickAddView { [weak self] title in
            self?.model.add(title)
        }
        menu.addItem(add)
        menu.addItem(.separator())

        // Up to 5 open todos, creation order (matches the MCP `list` default).
        let open = model.openTodos
        for todo in open.prefix(5) {
            let item = NSMenuItem(title: todo.title, action: #selector(toggleTodo(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = todo.id as NSCopying
            item.image = NSImage(systemSymbolName: "circle", accessibilityDescription: nil)
            menu.addItem(item)
        }
        if open.count > 5 {
            let more = NSMenuItem(title: "… \(open.count - 5) more — open the app",
                                  action: nil, keyEquivalent: "")
            more.isEnabled = false
            menu.addItem(more)
        }
        if open.isEmpty {
            let none = NSMenuItem(title: "All clear", action: nil, keyEquivalent: "")
            none.isEnabled = false
            menu.addItem(none)
        }

        menu.addItem(.separator())
        let openApp = NSMenuItem(title: "Open Tandem", action: #selector(openApp), keyEquivalent: "t")
        openApp.target = self
        menu.addItem(openApp)
    }

    @objc private func toggleTodo(_ sender: NSMenuItem) {
        guard let id = sender.representedObject as? UUID else { return }
        model.toggle(id)
    }

    @objc private func openApp() {
        NSApp.activate(ignoringOtherApps: true)
        for window in NSApp.windows where window.isVisible || window.canBecomeMain {
            window.makeKeyAndOrderFront(nil)
        }
    }

    // MARK: - Quick-add field (NSTextField in a menu item view)

    private final class QuickAddView: NSView {
        private let field: NSTextField
        private let fieldDelegate: FieldDelegate

        init(onCommit: @escaping (String) -> Void) {
            fieldDelegate = FieldDelegate(onCommit: onCommit)
            field = NSTextField(frame: NSRect(x: 18, y: 7, width: 216, height: 22))
            super.init(frame: NSRect(x: 0, y: 0, width: 250, height: 36))
            field.placeholderString = "Add a todo…"
            field.font = .systemFont(ofSize: 13)
            field.delegate = fieldDelegate
            addSubview(field)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("prototype") }

        private final class FieldDelegate: NSObject, NSTextFieldDelegate {
            let onCommit: (String) -> Void

            init(onCommit: @escaping (String) -> Void) {
                self.onCommit = onCommit
            }

            func control(_ control: NSControl, textShouldEndEditing fieldEditor: NSText) -> Bool {
                onCommit((control as? NSTextField)?.stringValue ?? "")
                return true
            }

            func controlTextDidEndEditing(_ obj: Notification) {
                (obj.object as? NSTextField)?.stringValue = ""
            }
        }
    }

    // MARK: - Status item image (glyph, optional count badge, template)

    private static func statusImage(glyph: String, badged: Bool, openCount: Int) -> NSImage {
        let config = NSImage.SymbolConfiguration(pointSize: 15, weight: .regular)
        let base = NSImage(systemSymbolName: glyph, accessibilityDescription: "Tandem")?
            .withSymbolConfiguration(config)
            ?? NSImage(systemSymbolName: "circle", accessibilityDescription: "Tandem")!

        guard badged, openCount > 0 else {
            let img = NSImage(size: base.size, flipped: false) { _ in
                base.draw(at: .zero, from: .zero, operation: .sourceOver, fraction: 1)
                return true
            }
            img.isTemplate = true
            return img
        }

        let badge: CGFloat = 12
        let size = NSSize(width: base.size.width + 3, height: base.size.height + 3)
        let img = NSImage(size: size, flipped: false) { _ in
            base.draw(at: .zero, from: .zero, operation: .sourceOver, fraction: 1)
            let circle = NSRect(x: size.width - badge - 0.5, y: -0.5, width: badge, height: badge)
            NSColor.black.setFill()
            NSBezierPath(ovalIn: circle).fill()
            // Knock the count out of the badge (alpha = 0 → template cutout).
            let str = NSAttributedString(
                string: "\(min(openCount, 99))",
                attributes: [.font: NSFont.systemFont(ofSize: 8, weight: .bold)]
            )
            let ts = str.size()
            NSGraphicsContext.current?.cgContext.setBlendMode(.clear)
            str.draw(at: NSPoint(x: circle.midX - ts.width / 2, y: circle.midY - ts.height / 2))
            return true
        }
        img.isTemplate = true
        return img
    }
}
