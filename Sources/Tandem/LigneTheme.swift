import SwiftUI

// The Ligne Claire theme, reach B ("subtle Ligne Claire" — ticket 01 verdict,
// 2026-10-11): the theme lives in the chrome — flat-filled paper surfaces,
// uniform 1pt hairlines, flat-filled line-drawn controls, native body
// typography, illustration only in the empty state. The interface body stays
// native-clean; restraint is the style (no cel drop-shadows, no comic
// outlines, no gradients — those are the v0 failure modes).
//
// One shared scale: every control takes its metrics from here — no literals
// in control code.

// MARK: - Palette (final values — locked in ticket 03, 2026-10-11)

// "Ligne Claire, muted earth" — dusty blues, washed-out reds, sandy beiges.
// Light = the "day panel" (paper-like, the natural Ligne Claire register);
// dark = the "night panel" (blue-charcoal, linework inverts to warm paper).
// The app follows the system appearance — never forces one.
struct Palette: Equatable {
    // surfaces
    var paper: Color       // app background
    var parchment: Color   // raised surfaces / panels / banner
    var sand: Color        // secondary fill, hairlines, disabled
    var shadow: Color      // hard cel offset (flat, no blur) — reserved
    // linework + text
    var ink: Color         // uniform line weight, primary text
    var inkDim: Color      // secondary text (age, placeholder)
    // accents
    var dustyBlue: Color   // primary accent: selected filter, "Open Tandem"
    var dustyBlueDeep: Color  // hard-edge cel shade of dustyBlue
    var ember: Color       // secondary accent: focus, warning
    var emberDeep: Color
    var moss: Color        // done state
    var mossDeep: Color
    // illustration
    var sky: Color         // header band sky (reserved)
    var hill: Color        // geometric hill, near (reserved)
    var hillFar: Color     // geometric hill, far (reserved)
    var skin: Color        // riders
    var sun: Color         // sky disc (reserved)

    /// The app follows the system: day panel in light, night panel in dark.
    static func forScheme(_ scheme: ColorScheme) -> Palette {
        scheme == .dark ? .night : .day
    }
}

extension Palette {
    static let day = Palette(
        paper:       hex(0xF5F0E6),
        parchment:   hex(0xEAE2D0),
        sand:        hex(0xD9CDB4),
        shadow:      hex(0xCDBF9F),
        ink:         hex(0x2B2723),
        inkDim:      hex(0x8A8171),
        dustyBlue:   hex(0x7490A6),
        dustyBlueDeep: hex(0x58738A),
        ember:       hex(0xC47662),
        emberDeep:   hex(0xA85D4B),
        moss:        hex(0x8C9A78),
        mossDeep:    hex(0x6F7F5D),
        sky:         hex(0xCFDDE2),
        hill:        hex(0xB7C4AE),
        hillFar:     hex(0xC8CFBB),
        skin:        hex(0xE9CBA4),
        sun:         hex(0xE5C07B)
    )

    static let night = Palette(
        paper:       hex(0x23262B),
        parchment:   hex(0x2C3037),
        sand:        hex(0x3A3F47),
        shadow:      hex(0x1A1D21),
        ink:         hex(0xEDE5D4),
        inkDim:      hex(0x9A958A),
        dustyBlue:   hex(0x92AAC0),
        dustyBlueDeep: hex(0x6E8AA3),
        ember:       hex(0xC98D7A),
        emberDeep:   hex(0xA96E5C),
        moss:        hex(0x9CAB86),
        mossDeep:    hex(0x7A8A68),
        sky:         hex(0x2A3038),
        hill:        hex(0x3A4441),
        hillFar:     hex(0x313A3E),
        skin:        hex(0xE3C29B),
        sun:         hex(0xEDE5D4)
    )

    static func hex(_ v: UInt32) -> Color {
        Color(.sRGB,
              red: Double((v >> 16) & 0xFF) / 255,
              green: Double((v >> 8) & 0xFF) / 255,
              blue: Double(v & 0xFF) / 255)
    }
}

// MARK: - Shared scale

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
    static let card: CGFloat = 12       // banner
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
