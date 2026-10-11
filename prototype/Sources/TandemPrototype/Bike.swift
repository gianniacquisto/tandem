import SwiftUI

// PROTOTYPE — Ligne Claire tandem bike (ticket 01, draft asset 1 of 3).
//
// The brand motif: two riders, one machine — human + agent, one list.
// Uniform line weight, flat fills, hard-edge (cel) shading only, no
// gradients. Geometry lives in fixed design spaces and scales to fit; the
// stroke scales with the drawing so the line stays uniform.
//
// Drafts:
//   - drawScene(riders: true)  → illustration (empty state, header band, icon)
//   - drawScene(riders: false) → bare bike (menu bar glyph, parked moments)
//   - drawIcon                 → app icon tile candidate

enum BikeArt {
    // MARK: design spaces

    static let sceneSize = CGSize(width: 260, height: 170)

    // scene geometry
    static let rearAxle  = CGPoint(x: 62,  y: 118)
    static let frontAxle = CGPoint(x: 206, y: 118)
    static let wheelR: CGFloat = 34
    static let crank  = CGPoint(x: 132, y: 118)   // pilot crank (chainring 14)
    static let crank2 = CGPoint(x: 100, y: 118)   // stoker crank (r 10)
    static let head    = CGPoint(x: 190, y: 60)
    static let seatPilot  = CGPoint(x: 152, y: 58)
    static let seatStoker = CGPoint(x: 94,  y: 60)

    // MARK: drawing entry points

    /// Bike (+ riders) centred in `size`, scaled to fit.
    static func drawScene(_ ctx: inout GraphicsContext, size: CGSize, palette: Palette,
                          riders: Bool = true, lw: CGFloat = 3.5) {
        let s = min(size.width / sceneSize.width, size.height / sceneSize.height)
        var c = ctx
        c.translateBy(x: (size.width - sceneSize.width * s) / 2,
                      y: (size.height - sceneSize.height * s) / 2)
        c.scaleBy(x: s, y: s)
        drawBike(&c, palette: palette, lw: lw, riders: riders)
    }

    /// App icon tile: flat sky, one sun/moon disc, two geometric untextured
    /// hills, the tandem on the near hill. Rounded-square tile.
    static func drawIcon(_ ctx: inout GraphicsContext, size: CGSize, palette: Palette) {
        let lw = size.width * 0.011
        let tile = Path(roundedRect: CGRect(origin: .zero, size: size),
                        cornerRadius: size.width * 0.225)
        ctx.clip(to: tile)
        fill(&ctx, tile, palette.sky)

        // sun / moon
        let sun = circle(CGPoint(x: size.width * 0.8, y: size.height * 0.2), size.width * 0.075)
        fill(&ctx, sun, palette.sun)
        stroke(&ctx, sun, palette.ink, lw)

        // far hill (left)
        let far = Path { p in
            p.move(to: CGPoint(x: 0, y: size.height))
            p.addLine(to: CGPoint(x: 0, y: size.height * 0.68))
            p.addQuadCurve(to: CGPoint(x: size.width * 0.62, y: size.height),
                           control: CGPoint(x: size.width * 0.24, y: size.height * 0.46))
            p.closeSubpath()
        }
        outline(&ctx, far, palette.hillFar, palette.ink, lw)

        // near hill (right)
        let near = Path { p in
            p.move(to: CGPoint(x: size.width, y: size.height))
            p.addLine(to: CGPoint(x: size.width * 0.34, y: size.height))
            p.addQuadCurve(to: CGPoint(x: size.width, y: size.height * 0.58),
                           control: CGPoint(x: size.width * 0.94, y: size.height * 0.4))
            p.closeSubpath()
        }
        outline(&ctx, near, palette.hill, palette.ink, lw)

        // tandem on the near hill (wheel bottoms planted on the hill crest)
        let s = size.width * 0.62 / sceneSize.width
        var b = ctx
        b.translateBy(x: size.width * 0.7 - sceneSize.width * s / 2,
                      y: size.height * 0.66 - 152 * s)
        b.scaleBy(x: s, y: s)
        drawBike(&b, palette: palette, lw: 3.5, riders: true)
    }

    // MARK: the machine

    private static func drawBike(_ ctx: inout GraphicsContext, palette: Palette,
                                 lw: CGFloat, riders: Bool) {
        let ink = palette.ink

        // wheels + hubs
        for c in [rearAxle, frontAxle] {
            stroke(&ctx, circle(c, wheelR), ink, lw)
            fill(&ctx, circle(c, 4.5), ink)
        }

        // cranks + drive chain
        stroke(&ctx, circle(crank, 14), ink, lw)
        stroke(&ctx, circle(crank2, 10), ink, lw * 0.8)
        stroke(&ctx, line([CGPoint(x: 100, y: 108), CGPoint(x: 132, y: 104)]), ink, lw * 0.65)
        stroke(&ctx, line([CGPoint(x: 100, y: 128), CGPoint(x: 132, y: 132)]), ink, lw * 0.65)

        // frame — one long top tube, two seat clusters
        for f in [
            line([rearAxle, seatStoker]),       // seat stay
            line([crank, rearAxle]),            // chain stay
            line([seatStoker, head]),           // top tube
            line([head, crank]),                // down tube
            line([seatPilot, crank]),           // pilot seat tube
            line([seatStoker, crank2]),         // stoker seat tube
            line([head, frontAxle]),            // fork
        ] {
            stroke(&ctx, f, ink, lw)
        }

        // saddles
        fill(&ctx, roundedRect(CGRect(x: 140, y: 53, width: 26, height: 6), 3), ink)
        fill(&ctx, roundedRect(CGRect(x: 82, y: 55, width: 26, height: 6), 3), ink)

        // handlebars (pilot, stoker)
        stroke(&ctx, line([CGPoint(x: 190, y: 60), CGPoint(x: 196, y: 50), CGPoint(x: 204, y: 50)]), ink, lw)
        stroke(&ctx, line([CGPoint(x: 136, y: 60), CGPoint(x: 140, y: 48), CGPoint(x: 148, y: 48)]), ink, lw)

        // pedals (pilot on chainring, stoker on stoker crank)
        fill(&ctx, roundedRect(CGRect(x: 136, y: 124, width: 12, height: 4), 2), ink)
        fill(&ctx, roundedRect(CGRect(x: 116, y: 104, width: 12, height: 4), 2), ink)
        fill(&ctx, roundedRect(CGRect(x: 102, y: 122, width: 11, height: 4), 2), ink)
        fill(&ctx, roundedRect(CGRect(x: 86, y: 106, width: 11, height: 4), 2), ink)

        if riders {
            drawStoker(&ctx, palette: palette, lw: lw)
            drawPilot(&ctx, palette: palette, lw: lw)
        }
    }

    // MARK: the riders

    /// Front rider — dusty blue jersey, on the pilot crank.
    private static func drawPilot(_ ctx: inout GraphicsContext, palette p: Palette, lw: CGFloat) {
        let ink = p.ink
        let limb = lw * 0.9

        // legs first (under torso): front leg → low pedal, rear leg → high pedal
        stroke(&ctx, line([CGPoint(x: 156, y: 78), CGPoint(x: 172, y: 100), CGPoint(x: 143, y: 124)]), ink, limb)
        stroke(&ctx, line([CGPoint(x: 152, y: 78), CGPoint(x: 158, y: 104), CGPoint(x: 124, y: 105)]), ink, limb)

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
        stroke(&ctx, line([CGPoint(x: 168, y: 46), CGPoint(x: 184, y: 56), CGPoint(x: 200, y: 51)]), ink, limb)

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
        stroke(&ctx, line([CGPoint(x: 98, y: 76), CGPoint(x: 114, y: 98), CGPoint(x: 108, y: 122)]), ink, limb)
        stroke(&ctx, line([CGPoint(x: 94, y: 76), CGPoint(x: 94, y: 102), CGPoint(x: 93, y: 107)]), ink, limb)

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
        stroke(&ctx, line([CGPoint(x: 92, y: 44), CGPoint(x: 116, y: 50), CGPoint(x: 146, y: 48)]), ink, limb)

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
    var lineWidth: CGFloat = 3.5

    var body: some View {
        Canvas { ctx, size in
            BikeArt.drawScene(&ctx, size: size, palette: palette, riders: riders, lw: lineWidth)
        }
    }
}

/// App icon tile view (see `BikeArt.drawIcon`).
struct BikeIcon: View {
    var palette: Palette

    var body: some View {
        Canvas { ctx, size in
            BikeArt.drawIcon(&ctx, size: size, palette: palette)
        }
    }
}
