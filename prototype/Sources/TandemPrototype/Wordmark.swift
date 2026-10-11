import SwiftUI

// PROTOTYPE — wordmark treatments (ticket 01, draft asset 2 of 3).
//
// Three candidates for "Tandem", to be judged in the Assets face and locked
// in ticket 03:
//   1. .lineLettering — custom line lettering, uniform Ligne Claire stroke
//   2. .displayFace   — clean geometric display face (Avenir Next Demi Bold)
//   3. .hybrid        — both: display face wordmark riding a line-drawn bike
//                       on its baseline
//
// The 8-bit pixel font is dead (map, 2026-10-11).

enum WordmarkTreatment: String, CaseIterable, Identifiable {
    case lineLettering, displayFace, hybrid
    var id: String { rawValue }

    var title: String {
        switch self {
        case .lineLettering: "1 · Line lettering"
        case .displayFace:   "2 · Display face"
        case .hybrid:        "3 · Hybrid (face + bike baseline)"
        }
    }
}

/// Treatment 1 — "TANDEM" in custom line lettering.
/// Design space 320×72; cap height 44; uniform round-capped stroke.
struct LineLettering: View {
    var palette: Palette

    var body: some View {
        Canvas { ctx, size in
            let s = min(size.width / 320, size.height / 72)
            var c = ctx
            c.translateBy(x: (size.width - 320 * s) / 2, y: (size.height - 72 * s) / 2)
            c.scaleBy(x: s, y: s)
            Self.draw(&c, color: palette.ink, lw: 7)
        }
    }

    static func draw(_ ctx: inout GraphicsContext, color: Color, lw: CGFloat) {
        func pt(_ px: CGFloat, _ py: CGFloat) -> CGPoint { CGPoint(x: px, y: py) }
        func seg(_ pts: [CGPoint]) {
            BikeArt.stroke(&ctx, BikeArt.line(pts), color, lw)
        }

        var x: CGFloat = 28
        // T — width 34
        seg([pt(x + 17, 14), pt(x + 17, 58)])
        seg([pt(x + 1, 14), pt(x + 33, 14)])
        x += 44
        // A — width 36
        seg([pt(x + 2, 58), pt(x + 18, 14), pt(x + 34, 58)])
        seg([pt(x + 8, 44), pt(x + 28, 44)])
        x += 46
        // N — width 36
        seg([pt(x + 2, 58), pt(x + 2, 14)])
        seg([pt(x + 2, 14), pt(x + 34, 58)])
        seg([pt(x + 34, 58), pt(x + 34, 14)])
        x += 46
        // D — width 36
        seg([pt(x + 2, 14), pt(x + 2, 58)])
        var d = Path()
        d.move(to: pt(x + 2, 14))
        d.addLine(to: pt(x + 18, 14))
        d.addCurve(to: pt(x + 18, 58),
                   control1: pt(x + 36, 16),
                   control2: pt(x + 36, 56))
        d.addLine(to: pt(x + 2, 58))
        BikeArt.stroke(&ctx, d, color, lw)
        x += 46
        // E — width 32
        seg([pt(x + 2, 14), pt(x + 2, 58)])
        seg([pt(x + 2, 14), pt(x + 30, 14)])
        seg([pt(x + 2, 36), pt(x + 26, 36)])
        seg([pt(x + 2, 58), pt(x + 30, 58)])
        x += 42
        // M — width 40
        seg([pt(x + 2, 58), pt(x + 2, 14)])
        seg([pt(x + 2, 14), pt(x + 20, 44)])
        seg([pt(x + 20, 44), pt(x + 38, 14)])
        seg([pt(x + 38, 14), pt(x + 38, 58)])
    }
}

/// Treatment 3's baseline: the bare bike "riding" under the wordmark.
struct BikeBaseline: View {
    var palette: Palette

    var body: some View {
        Canvas { ctx, size in
            // hairline baseline, interrupted by the bike
            let midY = size.height / 2
            let bikeW = size.height * (BikeArt.sceneSize.width / BikeArt.sceneSize.height)
            let bikeX = (size.width - bikeW) / 2
            var line = Path()
            line.move(to: CGPoint(x: 0, y: midY))
            line.addLine(to: CGPoint(x: bikeX - 6, y: midY))
            var r = line
            r.move(to: CGPoint(x: bikeX + bikeW + 6, y: midY))
            r.addLine(to: CGPoint(x: size.width, y: midY))
            BikeArt.stroke(&ctx, line, palette.ink, Line.fine)
            BikeArt.stroke(&ctx, r, palette.ink, Line.fine)
            // the bike, wheel bottoms on the baseline
            let s = (size.height - 6) / BikeArt.sceneSize.height
            var b = ctx
            b.translateBy(x: bikeX + (bikeW - BikeArt.sceneSize.width * s) / 2,
                          y: midY + 3 - 152 * s)
            b.scaleBy(x: s, y: s)
            BikeArt.drawScene(&b, size: BikeArt.sceneSize, palette: palette, riders: false, lw: 5)
        }
    }
}

struct Wordmark: View {
    var treatment: WordmarkTreatment
    var palette: Palette
    /// Height of the lettering block (hybrid reserves room for the bike below).
    var height: CGFloat = 34

    var body: some View {
        switch treatment {
        case .lineLettering:
            LineLettering(palette: palette)
                .frame(height: height)
        case .displayFace:
            Text("Tandem")
                .font(.custom("AvenirNext-DemiBold", size: height * 1.25))
                .foregroundStyle(palette.ink)
                .tracking(0.5)
        case .hybrid:
            VStack(spacing: 1) {
                Text("Tandem")
                    .font(.custom("AvenirNext-DemiBold", size: height * 1.25))
                    .foregroundStyle(palette.ink)
                    .tracking(0.5)
                BikeBaseline(palette: palette)
                    .frame(width: height * 5.2, height: height * 0.62)
            }
        }
    }
}
