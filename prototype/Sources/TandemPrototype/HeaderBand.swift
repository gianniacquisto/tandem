import SwiftUI

// PROTOTYPE — the illustrated header band (reach C only).
//
// v2 redo: quiet, not a scene. Flat sky, one far hill and one near hill as
// single geometric curves (crest stroke only — no closed comic outline),
// and a small bare bike parked on the near crest. No sun, no riders, no
// text. Generous negative space.

struct HeaderBand: View {
    var palette: Palette

    static let bandHeight: CGFloat = 96

    var body: some View {
        Canvas { ctx, size in
            let w = size.width
            let h = size.height

            // sky
            BikeArt.fill(&ctx, Path(CGRect(origin: .zero, size: size)), palette.sky)

            // far hill — one open curve, left of centre
            let farStart = CGPoint(x: 0, y: h * 0.58)
            let farEnd = CGPoint(x: w * 0.62, y: h)
            let farCtl = CGPoint(x: w * 0.28, y: h * 0.42)
            var farFill = Path()
            farFill.move(to: CGPoint(x: 0, y: h))
            farFill.addLine(to: farStart)
            farFill.addQuadCurve(to: farEnd, control: farCtl)
            farFill.closeSubpath()
            BikeArt.fill(&ctx, farFill, palette.hillFar)
            var farCrest = Path()
            farCrest.move(to: farStart)
            farCrest.addQuadCurve(to: farEnd, control: farCtl)
            BikeArt.stroke(&ctx, farCrest, palette.ink, Line.hairline)

            // near hill — one open curve, right of centre
            let nearStart = CGPoint(x: w * 0.34, y: h)
            let nearEnd = CGPoint(x: w, y: h * 0.46)
            let nearCtl = CGPoint(x: w * 0.70, y: h * 0.24)
            var nearFill = Path()
            nearFill.move(to: CGPoint(x: w, y: h))
            nearFill.addLine(to: nearEnd)
            nearFill.addQuadCurve(to: nearStart, control: nearCtl)
            nearFill.closeSubpath()
            BikeArt.fill(&ctx, nearFill, palette.hill)
            var nearCrest = Path()
            nearCrest.move(to: nearStart)
            nearCrest.addQuadCurve(to: nearEnd, control: nearCtl)
            BikeArt.stroke(&ctx, nearCrest, palette.ink, Line.hairline)

            // small bare bike on the near crest
            let t: CGFloat = 0.55
            let mt = 1 - t
            let crest = CGPoint(
                x: mt * mt * nearStart.x + 2 * mt * t * nearCtl.x + t * t * nearEnd.x,
                y: mt * mt * nearStart.y + 2 * mt * t * nearCtl.y + t * t * nearEnd.y
            )
            let s = 42.0 / (BikeArt.inkBottom - BikeArt.inkTop)
            var b = ctx
            b.translateBy(x: crest.x - 134 * s, y: crest.y - BikeArt.inkBottom * s)
            b.scaleBy(x: s, y: s)
            BikeArt.drawScene(&b, size: BikeArt.sceneSize, palette: palette, riders: false)

            // hairline into the content
            BikeArt.stroke(
                &ctx,
                BikeArt.line([CGPoint(x: 0, y: h - Line.hairline / 2), CGPoint(x: w, y: h - Line.hairline / 2)]),
                palette.sand, Line.hairline
            )
        }
        .frame(height: Self.bandHeight)
    }
}
