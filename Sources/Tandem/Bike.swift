import SwiftUI

// The Ligne Claire tandem bike: two riders, one machine — human + agent, one
// list. Flat fills, uniform ink outline, no gradients.
//
// Placement rule (ticket 01 handoff, updated by ticket 03): the bare bike
// with its double saddle is the brand mark — menu bar glyph and popover
// header. The main window's empty states now wear the brand photo
// (BrandAsset), with this vector scene as the fallback when the asset is
// missing.
//
// Geometry lives in a fixed design space and scales to fit; the stroke scales
// with the drawing so the line stays uniform.

enum BikeArt {
    // MARK: design space

    static let sceneSize = CGSize(width: 260, height: 170)

    /// Wheel bottoms in design space (the ground hairline sits here).
    static let inkBottom: CGFloat = 152

    // scene geometry
    static let rearAxle  = CGPoint(x: 58,  y: 118)
    static let frontAxle = CGPoint(x: 204, y: 118)
    static let wheelR: CGFloat = 34
    static let crankPilot  = CGPoint(x: 134, y: 118)  // chainring r 12
    static let crankStoker = CGPoint(x: 106, y: 118)  // stoker crank r 8
    static let head        = CGPoint(x: 192, y: 58)
    static let seatPilot   = CGPoint(x: 150, y: 56)
    static let seatStoker  = CGPoint(x: 92,  y: 58)

    // MARK: drawing entry point

    /// Bike (+ riders) centred in `size`, scaled to fit. `ground` draws a
    /// short quiet hairline under the wheels (parked moments only).
    static func drawScene(_ ctx: inout GraphicsContext, size: CGSize, palette: Palette,
                          riders: Bool = true, lw: CGFloat = Line.illustration,
                          ground: Bool = false) {
        let s = min(size.width / sceneSize.width, size.height / sceneSize.height)
        var c = ctx
        c.translateBy(x: (size.width - sceneSize.width * s) / 2,
                      y: (size.height - sceneSize.height * s) / 2)
        c.scaleBy(x: s, y: s)
        if ground {
            stroke(&c, line([CGPoint(x: 18, y: inkBottom), CGPoint(x: 242, y: inkBottom)]),
                   palette.sand, lw * 0.45)
        }
        drawBike(&c, palette: palette, lw: lw, riders: riders)
    }

    // MARK: the machine

    /// The bare bike: one long top tube, two saddles (the tandem tell),
    /// two handlebars, one chainring + stoker crank with four pedals.
    private static func drawBike(_ ctx: inout GraphicsContext, palette: Palette,
                                 lw: CGFloat, riders: Bool) {
        let ink = palette.ink

        // wheels + hubs
        for c in [rearAxle, frontAxle] {
            stroke(&ctx, circle(c, wheelR), ink, lw)
            fill(&ctx, circle(c, 4.5), ink)
        }

        // cranks
        stroke(&ctx, circle(crankPilot, 12), ink, lw)
        stroke(&ctx, circle(crankStoker, 8), ink, lw * 0.8)

        // frame — one long top tube, two seat clusters
        for f in [
            line([rearAxle, seatStoker]),       // seat stay
            line([crankPilot, rearAxle]),       // chain stay
            line([seatStoker, head]),           // top tube (the tandem tell)
            line([head, crankPilot]),           // down tube
            line([seatPilot, crankPilot]),      // pilot seat tube
            line([seatStoker, crankStoker]),    // stoker seat tube
            line([head, frontAxle]),            // fork
        ] {
            stroke(&ctx, f, ink, lw)
        }

        // saddles — flat ink slabs, the double saddle reads as "tandem"
        fill(&ctx, roundedRect(CGRect(x: 138, y: 51, width: 26, height: 6), 3), ink)
        fill(&ctx, roundedRect(CGRect(x: 80, y: 53, width: 26, height: 6), 3), ink)

        // handlebars (pilot at the head, stoker ahead of its saddle)
        stroke(&ctx, line([head, CGPoint(x: 198, y: 46), CGPoint(x: 206, y: 46)]), ink, lw)
        stroke(&ctx, line([CGPoint(x: 104, y: 58), CGPoint(x: 110, y: 46), CGPoint(x: 118, y: 46)]), ink, lw)

        // pedals — two per crank
        fill(&ctx, roundedRect(CGRect(x: 136, y: 124, width: 12, height: 4), 2), ink)
        fill(&ctx, roundedRect(CGRect(x: 122, y: 108, width: 12, height: 4), 2), ink)
        fill(&ctx, roundedRect(CGRect(x: 108, y: 124, width: 11, height: 4), 2), ink)
        fill(&ctx, roundedRect(CGRect(x: 95,  y: 108, width: 11, height: 4), 2), ink)

        if riders {
            drawStoker(&ctx, palette: palette, lw: lw)
            drawPilot(&ctx, palette: palette, lw: lw)
        }
    }

    // MARK: the riders (large, empty-state only)

    /// Front rider — dusty blue jersey, on the pilot crank.
    private static func drawPilot(_ ctx: inout GraphicsContext, palette p: Palette, lw: CGFloat) {
        let ink = p.ink
        let limb = lw * 0.9

        // legs first (under torso): front leg → low pedal, rear leg → high pedal
        stroke(&ctx, line([CGPoint(x: 156, y: 78), CGPoint(x: 172, y: 100), CGPoint(x: 143, y: 124)]), ink, limb)
        stroke(&ctx, line([CGPoint(x: 152, y: 78), CGPoint(x: 158, y: 104), CGPoint(x: 128, y: 110)]), ink, limb)

        // torso (flat jersey, ink-outlined)
        let torso = Path { t in
            t.move(to: CGPoint(x: 156, y: 40))
            t.addLine(to: CGPoint(x: 172, y: 44))
            t.addLine(to: CGPoint(x: 162, y: 80))
            t.addLine(to: CGPoint(x: 148, y: 76))
            t.closeSubpath()
        }
        outline(&ctx, torso, p.dustyBlue, ink, lw * 0.75)

        // arm to the pilot bar
        stroke(&ctx, line([CGPoint(x: 168, y: 46), CGPoint(x: 188, y: 54), CGPoint(x: 204, y: 46)]), ink, limb)

        // head + hair
        fill(&ctx, circle(CGPoint(x: 168, y: 27), 8.5), p.skin)
        stroke(&ctx, circle(CGPoint(x: 168, y: 27), 8.5), ink, lw * 0.75)
        var hair = Path()
        hair.addArc(center: CGPoint(x: 168, y: 26), radius: 9,
                    startAngle: .degrees(200), endAngle: .degrees(340), clockwise: false)
        stroke(&ctx, hair, ink, lw * 0.9)
    }

    /// Rear rider — ember jersey, on the stoker crank.
    private static func drawStoker(_ ctx: inout GraphicsContext, palette p: Palette, lw: CGFloat) {
        let ink = p.ink
        let limb = lw * 0.9

        // legs: front leg → low pedal, rear leg → high pedal
        stroke(&ctx, line([CGPoint(x: 98, y: 76), CGPoint(x: 112, y: 98), CGPoint(x: 109, y: 124)]), ink, limb)
        stroke(&ctx, line([CGPoint(x: 94, y: 76), CGPoint(x: 92, y: 102), CGPoint(x: 101, y: 110)]), ink, limb)

        // torso
        let torso = Path { t in
            t.move(to: CGPoint(x: 82, y: 38))
            t.addLine(to: CGPoint(x: 96, y: 42))
            t.addLine(to: CGPoint(x: 104, y: 78))
            t.addLine(to: CGPoint(x: 90, y: 74))
            t.closeSubpath()
        }
        outline(&ctx, torso, p.ember, ink, lw * 0.75)

        // arm to the stoker bar
        stroke(&ctx, line([CGPoint(x: 92, y: 44), CGPoint(x: 106, y: 50), CGPoint(x: 117, y: 46)]), ink, limb)

        // head + hair
        fill(&ctx, circle(CGPoint(x: 88, y: 25), 8.5), p.skin)
        stroke(&ctx, circle(CGPoint(x: 88, y: 25), 8.5), ink, lw * 0.75)
        var hair = Path()
        hair.addArc(center: CGPoint(x: 88, y: 24), radius: 9,
                    startAngle: .degrees(200), endAngle: .degrees(340), clockwise: false)
        stroke(&ctx, hair, ink, lw * 0.9)
    }

    // MARK: helpers

    static func circle(_ c: CGPoint, _ r: CGFloat) -> Path {
        Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r, width: 2 * r, height: 2 * r))
    }

    static func line(_ pts: [CGPoint]) -> Path {
        var p = Path()
        guard let first = pts.first else { return p }
        p.move(to: first)
        for pt in pts.dropFirst() { p.addLine(to: pt) }
        return p
    }

    static func roundedRect(_ rect: CGRect, _ r: CGFloat) -> Path {
        Path(roundedRect: rect, cornerRadius: r)
    }

    static func fill(_ ctx: inout GraphicsContext, _ path: Path, _ color: Color) {
        ctx.fill(path, with: .color(color))
    }

    static func stroke(_ ctx: inout GraphicsContext, _ path: Path, _ color: Color, _ w: CGFloat) {
        ctx.stroke(path, with: .color(color),
                   style: StrokeStyle(lineWidth: w, lineCap: .round, lineJoin: .round))
    }

    static func outline(_ ctx: inout GraphicsContext, _ path: Path,
                        _ fillC: Color, _ lineC: Color, _ w: CGFloat) {
        ctx.fill(path, with: .color(fillC))
        ctx.stroke(path, with: .color(lineC),
                   style: StrokeStyle(lineWidth: w, lineCap: .round, lineJoin: .round))
    }
}

/// Convenience view wrapper around `BikeArt.drawScene`.
struct BikeScene: View {
    var palette: Palette
    var riders: Bool = true
    var lineWidth: CGFloat = Line.illustration
    var ground: Bool = false

    var body: some View {
        Canvas { ctx, size in
            BikeArt.drawScene(&ctx, size: size, palette: palette,
                              riders: riders, lw: lineWidth, ground: ground)
        }
    }
}
