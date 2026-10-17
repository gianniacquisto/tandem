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

// MARK: - Palette (drawn from the brand asset — Resources/TandemBike.jpg)

// The values are sampled from the tandem-bike illustration: warm cream paper,
// a sage-green frame (the primary accent), rust saddles and grips (the
// secondary accent), and dark charcoal tires. The interface reads as the
// drawing, but at the calm register of a notes app (Obsidian/Notion
// conventions, 2026-10-17): structure is carried by low-contrast hairlines
// (`line`), not heavy ink outlines, and selection is a soft wash rather than a
// saturated fill. Rust marks what wants attention; sage is reserved for brand
// and the one primary action.
//
// Light = the "day panel" (paper-like, the natural Ligne Claire register);
// dark = the "night panel" (warm charcoal, linework inverts to paper cream).
// The app follows the system appearance — never forces one.
struct Palette: Equatable {
    // surfaces
    var paper: Color       // app background
    var parchment: Color   // raised surfaces / panels / selected segment
    var sand: Color        // secondary fill, dividers, disabled
    var shadow: Color      // hard cel offset (flat, no blur) — reserved
    // linework + text
    var line: Color        // control borders — low contrast, calm
    var ink: Color         // primary text
    var inkDim: Color      // secondary text (placeholder, unselected)
    // accents
    var accent: Color      // primary accent (frame sage): the one primary action
    var accentDeep: Color  // hard-edge cel shade of accent
    var ember: Color       // secondary accent (saddle rust): focus, warning
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
        paper:       hex(0xF1E9D7),
        parchment:   hex(0xE7DECA),
        sand:        hex(0xD6C9AE),
        shadow:      hex(0xC6B897),
        line:        hex(0xC6B99E),
        ink:         hex(0x211D18),
        inkDim:      hex(0x877D6B),
        accent:      hex(0x6E7D6E),
        accentDeep:  hex(0x55604F),
        ember:       hex(0xA9745C),
        emberDeep:   hex(0x8B5A45),
        moss:        hex(0x6C7A66),
        mossDeep:    hex(0x7E8D77),
        sky:         hex(0xCFDDE2),
        hill:        hex(0xB7C4AE),
        hillFar:     hex(0xC8CFBB),
        skin:        hex(0xE4C9A2),
        sun:         hex(0xE5C07B)
    )

    static let night = Palette(
        paper:       hex(0x23211C),
        parchment:   hex(0x2C2A23),
        sand:        hex(0x3C3930),
        shadow:      hex(0x191712),
        line:        hex(0x494437),
        ink:         hex(0xEDE6D3),
        inkDim:      hex(0x9C9583),
        accent:      hex(0x93A48F),
        accentDeep:  hex(0x6E7E6A),
        ember:       hex(0xC08A70),
        emberDeep:   hex(0x9E6B54),
        moss:        hex(0x8FA084),
        mossDeep:    hex(0x74856E),
        sky:         hex(0x2A2E28),
        hill:        hex(0x3A4441),
        hillFar:     hex(0x313A36),
        skin:        hex(0xE3C29B),
        sun:         hex(0xEDE6D3)
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
    static let control: CGFloat = 32    // one height: tab, filter, quick-add, buttons
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
    /// The checkbox tick: a short, springy pop as the mark is drawn — the
    /// "click" of checking something off. Slightly under-damped so it
    /// overshoots once and settles.
    static let check: Animation = .spring(response: 0.3, dampingFraction: 0.6)
}
