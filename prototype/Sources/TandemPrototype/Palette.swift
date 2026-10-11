import SwiftUI

// PROTOTYPE — draft candidate palette for ticket 01 (asset 3 of 3).
//
// "Ligne Claire, muted earth" — dusty blues, washed-out reds, sandy beiges.
// Flat fills, hard-edged (cel) shading only, no gradients. Named for use in
// the spec; values are candidates for human approval, not a lock.
//
// Light = the "day panel": paper-like, the natural Ligne Claire register.
// Dark = the "night panel": blue-charcoal, linework inverts to warm paper.

struct Palette: Equatable {
    // surfaces
    var paper: Color       // app background
    var parchment: Color   // raised surfaces / panels / banner
    var sand: Color        // secondary fill, hairlines, disabled
    var shadow: Color      // hard cel offset (flat, no blur, no gradient)
    // linework + text
    var ink: Color         // uniform line weight, primary text
    var inkDim: Color      // secondary text (age, placeholder)
    // accents
    var dustyBlue: Color   // primary accent: selected filter, links
    var dustyBlueDeep: Color  // hard-edge cel shade of dustyBlue
    var ember: Color       // secondary accent: focus, warning
    var emberDeep: Color
    var moss: Color        // done state
    var mossDeep: Color
    // illustration
    var sky: Color         // header band sky
    var hill: Color        // geometric hill, near
    var hillFar: Color     // geometric hill, far (hard-edge tone shift)
    var skin: Color        // riders
    var sun: Color         // sky disc (sun / moon)
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

/// Named entries, for the swatch grid in the Assets face.
extension Palette {
    struct Swatch: Identifiable {
        let id = UUID()
        let name: String
        let role: String
        let value: Color
        let hexLabel: String
    }

    var swatches: [Swatch] {
        [
            .init(name: "Paper", role: "background", value: paper, hexLabel: Self.label(paper, light: self == Self.day)),
            .init(name: "Parchment", role: "raised surfaces", value: parchment, hexLabel: Self.label(parchment, light: self == Self.day)),
            .init(name: "Sand", role: "hairlines · secondary", value: sand, hexLabel: Self.label(sand, light: self == Self.day)),
            .init(name: "Shadow", role: "hard cel offset", value: shadow, hexLabel: Self.label(shadow, light: self == Self.day)),
            .init(name: "Ink", role: "linework · text", value: ink, hexLabel: Self.label(ink, light: self == Self.day)),
            .init(name: "Dusty blue", role: "primary accent", value: dustyBlue, hexLabel: Self.label(dustyBlue, light: self == Self.day)),
            .init(name: "Dusty blue, deep", role: "cel shade", value: dustyBlueDeep, hexLabel: Self.label(dustyBlueDeep, light: self == Self.day)),
            .init(name: "Ember", role: "focus · warning", value: ember, hexLabel: Self.label(ember, light: self == Self.day)),
            .init(name: "Ember, deep", role: "cel shade", value: emberDeep, hexLabel: Self.label(emberDeep, light: self == Self.day)),
            .init(name: "Moss", role: "done state", value: moss, hexLabel: Self.label(moss, light: self == Self.day)),
            .init(name: "Moss, deep", role: "cel shade", value: mossDeep, hexLabel: Self.label(mossDeep, light: self == Self.day)),
            .init(name: "Sky", role: "illustration", value: sky, hexLabel: Self.label(sky, light: self == Self.day)),
            .init(name: "Hill", role: "illustration", value: hill, hexLabel: Self.label(hill, light: self == Self.day)),
            .init(name: "Hill, far", role: "illustration", value: hillFar, hexLabel: Self.label(hillFar, light: self == Self.day)),
            .init(name: "Sun / moon", role: "illustration", value: sun, hexLabel: Self.label(sun, light: self == Self.day)),
        ]
    }

    // Best-effort hex readback for the swatch labels (prototype aid).
    private static func label(_ c: Color, light: Bool) -> String {
        guard let ns = NSColor(c).usingColorSpace(.sRGB) else { return "—" }
        let r = Int((ns.redComponent * 255).rounded())
        let g = Int((ns.greenComponent * 255).rounded())
        let b = Int((ns.blueComponent * 255).rounded())
        return String(format: "#%02X%02X%02X", r, g, b)
    }
}

/// Uniform Ligne Claire line weights (draft).
enum Line {
    static let fine: CGFloat = 1.0    // hairlines, dividers
    static let standard: CGFloat = 1.5 // controls, rows
    static let bold: CGFloat = 2.5     // illustrations, wordmark
}
