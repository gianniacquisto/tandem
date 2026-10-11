import SwiftUI

// PROTOTYPE — draft-asset gallery (ticket 01 deliverables 1–3):
//   1. tandem-bike illustration (icon + glyph + empty state source)
//   2. app icon candidate
//   3. menu bar template glyph
//   4. wordmark treatments (3 candidates × light + dark)
//   5. candidate palette, named, light + dark values
//
// Everything here is a draft for human reaction; ticket 03 locks the finals.

struct AssetsFaceView: View {
    var model: TandemModel

    var body: some View {
        let day = Palette.day
        let night = Palette.night

        ScrollView {
            VStack(alignment: .leading, spacing: 30) {

                section("1 · Tandem-bike illustration") {
                    HStack(spacing: 16) {
                        pairTile(dayView: BikeScene(palette: day, riders: true),
                                 nightView: BikeScene(palette: night, riders: true))
                        pairTile(dayView: BikeScene(palette: day, riders: false),
                                 nightView: BikeScene(palette: night, riders: false),
                                 dayLabel: "day — bare bike (glyph, empty state)",
                                 nightLabel: "night — bare bike")
                    }
                    note("Two riders, one machine — human + agent, one list. The same drawing feeds the app icon, the menu bar glyph (bare bike), and the empty states.")
                }

                section("2 · App icon candidate") {
                    HStack(spacing: 16) {
                        HStack(alignment: .bottom, spacing: 8) {
                            BikeIcon(palette: day).frame(width: 96, height: 96)
                            BikeIcon(palette: day).frame(width: 56, height: 56)
                            BikeIcon(palette: day).frame(width: 32, height: 32)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                        .background(RoundedRectangle(cornerRadius: 12).fill(day.paper)
                            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(day.ink, lineWidth: Line.standard)))
                        HStack(alignment: .bottom, spacing: 8) {
                            BikeIcon(palette: night).frame(width: 96, height: 96)
                            BikeIcon(palette: night).frame(width: 56, height: 56)
                            BikeIcon(palette: night).frame(width: 32, height: 32)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                        .background(RoundedRectangle(cornerRadius: 12).fill(night.paper)
                            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(night.ink, lineWidth: Line.standard)))
                    }
                    note("Flat sky, one sun/moon, two geometric untextured hills, the tandem on the near hill. Feeds Scripts/make-app-icon.sh → Resources/AppIcon.icns at final lock (ticket 03).")
                }

                section("3 · Menu bar glyph (template-rendered)") {
                    HStack(spacing: 16) {
                        glyphStrip(background: Color(white: 0.96), glyph: Palette.day.ink)
                        glyphStrip(background: Color(white: 0.09), glyph: Palette.night.ink)
                    }
                    note("Bare bike, template image (system tints it). The double saddle is the tandem tell. Overrides v1's checklist glyph per the v2 map.")
                }

                section("4 · Wordmark treatments") {
                    VStack(spacing: 14) {
                        ForEach(WordmarkTreatment.allCases) { t in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(t.title)
                                    .font(.system(size: 12, weight: .medium, design: .monospaced))
                                    .foregroundStyle(day.inkDim)
                                HStack(spacing: 16) {
                                    wordmarkCard(day, t)
                                    wordmarkCard(night, t)
                                }
                            }
                        }
                    }
                    note("Candidate type only — the final wordmark, display face, and any bundled font license are locked in ticket 03.")
                }

                section("5 · Candidate palette") {
                    HStack(alignment: .top, spacing: 16) {
                        swatchGrid("Day panel", day)
                        swatchGrid("Night panel", night)
                    }
                    note("Named candidates: dusty blues, washed-out reds, sandy beiges. Usage: Paper = background, Ink = uniform linework + text, Dusty blue = primary accent, Ember = focus/warning, Moss = done state. Final values lock in ticket 03.")
                }
            }
            .padding(24)
        }
        .background(day.paper)
    }

    // MARK: layout helpers

    private func section<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title.uppercased())
                .font(.system(size: 11, weight: .semibold, design: .monospaced))
                .tracking(1.2)
                .foregroundStyle(Palette.day.inkDim)
            content()
        }
    }

    private func note(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 11.5))
            .foregroundStyle(Palette.day.inkDim)
            .fixedSize(horizontal: false, vertical: true)
    }

    private func pairTile(dayView: some View, nightView: some View,
                          dayLabel: String = "day", nightLabel: String = "night") -> some View {
        let day = Palette.day
        let night = Palette.night
        return VStack(spacing: 8) {
            HStack(spacing: 16) {
                tile(day.paper, day.ink) { dayView }
                tile(night.paper, night.ink) { nightView }
            }
            HStack(spacing: 16) {
                Text(dayLabel).frame(maxWidth: .infinity)
                Text(nightLabel).frame(maxWidth: .infinity)
            }
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(Palette.day.inkDim)
        }
    }

    private func tile(_ bg: Color, _ border: Color, @ViewBuilder content: () -> some View) -> some View {
        content()
            .frame(width: 170, height: 112)
            .padding(8)
            .background(RoundedRectangle(cornerRadius: 10).fill(bg)
                .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(border, lineWidth: Line.standard)))
    }

    private func glyphStrip(background: Color, glyph: Color) -> some View {
        HStack(spacing: 18) {
            BikeScene(palette: Palette(paper: background, parchment: background, sand: background, shadow: background,
                                       ink: glyph, inkDim: glyph, dustyBlue: glyph, dustyBlueDeep: glyph,
                                       ember: glyph, emberDeep: glyph, moss: glyph, mossDeep: glyph,
                                       sky: background, hill: background, hillFar: background,
                                       skin: glyph, sun: glyph),
                      riders: false, lineWidth: 5)
                .frame(width: 46, height: 30)
            Text("16 pt").font(.system(size: 10, design: .monospaced)).foregroundStyle(glyph.opacity(0.6))
        }
        .frame(maxWidth: .infinity)
        .frame(height: 44)
        .background(RoundedRectangle(cornerRadius: 10).fill(background))
    }

    private func wordmarkCard(_ pal: Palette, _ treatment: WordmarkTreatment) -> some View {
        Wordmark(treatment: treatment, palette: pal, height: 30)
            .frame(width: 240, height: 74)
            .background(RoundedRectangle(cornerRadius: 10).fill(pal.paper)
                .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(pal.ink, lineWidth: Line.standard)))
    }

    private func swatchGrid(_ title: String, _ pal: Palette) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(.system(size: 12, weight: .semibold)).foregroundStyle(pal.ink)
            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 8) {
                ForEach(pal.swatches) { s in
                    HStack(spacing: 8) {
                        RoundedRectangle(cornerRadius: 6)
                            .fill(s.value)
                            .frame(width: 26, height: 26)
                            .overlay(RoundedRectangle(cornerRadius: 6)
                                .strokeBorder(pal.ink.opacity(0.5), lineWidth: Line.fine))
                        VStack(alignment: .leading, spacing: 1) {
                            Text("\(s.name)  \(s.hexLabel)")
                                .font(.system(size: 10.5, weight: .medium, design: .monospaced))
                                .foregroundStyle(pal.ink)
                            Text(s.role)
                                .font(.system(size: 9.5))
                                .foregroundStyle(pal.inkDim)
                        }
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 6)
                    .background(RoundedRectangle(cornerRadius: 8).fill(pal.parchment))
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
