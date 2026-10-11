import SwiftUI

// PROTOTYPE — illustrated header band (reach variant C only).
// Expansive sky, two geometric untextured hills, a sun/moon disc, the tandem
// on the near hill. Flat fills, uniform ink outline, no gradients.

struct HeaderBand: View {
    var palette: Palette

    var body: some View {
        Canvas { ctx, size in
            let lw = Line.standard

            // sky
            BikeArt.fill(&ctx, Path(CGRect(origin: .zero, size: size)), palette.sky)

            // sun / moon (upper middle-right)
            let sun = BikeArt.circle(CGPoint(x: size.width * 0.62, y: size.height * 0.34), 11)
            BikeArt.fill(&ctx, sun, palette.sun)
            BikeArt.stroke(&ctx, sun, palette.ink, lw)

            // far hill (left, lower)
            let far = Path { p in
                p.move(to: CGPoint(x: 0, y: size.height))
                p.addLine(to: CGPoint(x: 0, y: size.height * 0.66))
                p.addQuadCurve(to: CGPoint(x: size.width * 0.68, y: size.height),
                               control: CGPoint(x: size.width * 0.3, y: size.height * 0.34))
                p.closeSubpath()
            }
            BikeArt.outline(&ctx, far, palette.hillFar, palette.ink, lw)

            // near hill (right, higher — the tandem rides its crest)
            let near = Path { p in
                p.move(to: CGPoint(x: size.width, y: size.height))
                p.addLine(to: CGPoint(x: size.width * 0.42, y: size.height))
                p.addQuadCurve(to: CGPoint(x: size.width, y: size.height * 0.44),
                               control: CGPoint(x: size.width * 0.82, y: size.height * 0.2))
                p.closeSubpath()
            }
            BikeArt.outline(&ctx, near, palette.hill, palette.ink, lw)

            // tandem on the near hill crest (wheel bottoms ≈ hill top at x ≈ 0.72w)
            let s = 46.0 / BikeArt.sceneSize.height
            var b = ctx
            b.translateBy(x: size.width * 0.72 - BikeArt.sceneSize.width * s / 2,
                          y: size.height * 0.72 - 152 * s)
            b.scaleBy(x: s, y: s)
            BikeArt.drawScene(&b, size: BikeArt.sceneSize, palette: palette, riders: true, lw: 3.5)
        }
    }
}
