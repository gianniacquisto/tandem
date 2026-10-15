import SwiftUI

// PROTOTYPE — the shared scale (ticket 01, v2 redo).
//
// v0's failure point: line weights, corner radii, and spacing were ad-hoc
// per control. Every control in the main window and the popover must take
// its metrics from here — no literals in control code.
//
// The Ligne Claire register at UI scale is restraint: one hairline, flat
// fills, no drop shadows (hard or soft), native body typography.

enum Line {
    /// Every UI border and divider: controls, checkbox, segments, banner.
    static let hairline: CGFloat = 1.0
    /// Illustration design-space stroke for the bike; scales with the drawing
    /// so the line stays uniform at any rendered size.
    static let illustration: CGFloat = 3.5
}

enum Radius {
    static let checkbox: CGFloat = 5    // line-drawn checkbox
    static let control: CGFloat = 9     // quick-add, filter, buttons
    static let card: CGFloat = 12       // banner, popover mock card
}

enum Space {
    static let gutter: CGFloat = 20     // main window horizontal inset
    static let row: CGFloat = 40        // main window row height
    static let control: CGFloat = 34    // main window control height
    static let popoverControl: CGFloat = 28
    static let popoverRow: CGFloat = 32
    static let popoverGutter: CGFloat = 12
    static let section: CGFloat = 12    // vertical rhythm between sections
}

enum Motion {
    /// Gentle: rows ease in, completions settle, clear-done eases out.
    static let insert: Animation = .easeOut(duration: 0.2)
    static let toggle: Animation = .easeInOut(duration: 0.18)
    static let remove: Animation = .easeIn(duration: 0.15)
}

// MARK: export preview
//
// ImageRenderer draws an offscreen TextField as a first-responder: the text
// area comes out as a solid system-yellow selection band with a caret
// (visible in the exported quick-add field). The running app is unaffected —
// only the export path is. In export mode LigneQuickAdd renders the
// placeholder text instead of a live TextField so previews show the intended
// look.

private struct TandemExportKey: EnvironmentKey { static let defaultValue = false }

extension EnvironmentValues {
    var tandemExport: Bool {
        get { self[TandemExportKey.self] }
        set { self[TandemExportKey.self] = newValue }
    }
}
