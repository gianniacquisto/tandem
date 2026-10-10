import AppKit
import TandemCore

/// The menu-bar item (spec user stories 12–19).
///
/// Built with AppKit (`NSStatusItem` + `NSMenu`) rather than SwiftUI's `MenuBarExtra`
/// because the menu needs a live quick-add text field, which SwiftUI's menu surface
/// can't host reliably — the same approach the approved prototype used (ticket 04).
///
/// Icon: SF Symbol `checklist`, template rendering, plain — no count badge.
/// Menu (rebuilt on every open, so it always reflects the store): quick-add field →
/// up to 5 open todos (tap to complete) → "… n more" / "All clear" → "Open Tandem".
final class MenuBarController: NSObject, NSMenuDelegate {
    private let model: TandemModel
    private let statusItem: NSStatusItem
    private let menu = NSMenu()

    init(model: TandemModel) {
        self.model = model
        self.statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()

        menu.autoenablesItems = false
        menu.delegate = self
        statusItem.menu = menu
        statusItem.button?.image = Self.icon
        statusItem.button?.toolTip = "Tandem"
    }

    deinit {
        NSStatusBar.system.removeStatusItem(statusItem)
    }

    // Plain `checklist` glyph, template rendering (adapts to light/dark menu bar),
    // no count badge (explicitly rejected in the prototype verdict).
    private static var icon: NSImage {
        let image = NSImage(systemSymbolName: "checklist", accessibilityDescription: "Tandem")
        image?.isTemplate = true
        return image ?? NSImage()
    }

    // MARK: - Menu (rebuilt on every open)

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()

        // Quick-add field at the top; Return commits.
        let add = NSMenuItem()
        add.view = QuickAddView { [weak self] title in
            self?.model.add(title)
        }
        menu.addItem(add)
        menu.addItem(.separator())

        let open = model.openTodos
        if open.isEmpty {
            menu.addItem(disabled("All clear"))
        } else {
            for todo in open.prefix(5) {
                let item = NSMenuItem(title: todo.title, action: #selector(completeTodo(_:)), keyEquivalent: "")
                item.target = self
                item.representedObject = todo.id.uuidString
                item.image = NSImage(systemSymbolName: "circle", accessibilityDescription: nil)
                menu.addItem(item)
            }
            if open.count > 5 {
                let more = NSMenuItem(title: "… \(open.count - 5) more — open Tandem",
                                      action: #selector(openApp), keyEquivalent: "")
                more.target = self
                menu.addItem(more)
            }
        }

        menu.addItem(.separator())

        let openApp = NSMenuItem(title: "Open Tandem", action: #selector(openApp), keyEquivalent: "t")
        openApp.target = self
        menu.addItem(openApp)
    }

    private func disabled(_ title: String) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        return item
    }

    @objc private func completeTodo(_ sender: NSMenuItem) {
        guard let id = sender.representedObject as? String else { return }
        model.complete(id)
    }

    @objc private func openApp() {
        NSApp.activate(ignoringOtherApps: true)
        WindowManager.shared.openMain?()
    }

    // MARK: - Quick-add field (an NSTextField hosted in a menu item view)

    private final class QuickAddView: NSView {
        private let field: NSTextField
        private let fieldDelegate: FieldDelegate

        init(onCommit: @escaping (String) -> Void) {
            fieldDelegate = FieldDelegate(onCommit: onCommit)
            field = NSTextField(frame: NSRect(x: 14, y: 8, width: 220, height: 22))
            super.init(frame: NSRect(x: 0, y: 0, width: 250, height: 38))
            field.placeholderString = "Add a todo…"
            field.font = .systemFont(ofSize: 13)
            field.delegate = fieldDelegate
            addSubview(field)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { fatalError("QuickAddView is not encoded") }
    }

    /// Commits the field's text when editing ends (Return) and clears it.
    private final class FieldDelegate: NSObject, NSTextFieldDelegate {
        let onCommit: (String) -> Void

        init(onCommit: @escaping (String) -> Void) {
            self.onCommit = onCommit
        }

        func control(_ control: NSControl, textShouldEndEditing fieldEditor: NSText) -> Bool {
            let text = ((control as? NSTextField)?.stringValue ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty { onCommit(text) }
            return true
        }

        func controlTextDidEndEditing(_ obj: Notification) {
            (obj.object as? NSTextField)?.stringValue = ""
        }
    }
}
