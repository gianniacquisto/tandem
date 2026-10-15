import SwiftUI
import AppKit

// PROTOTYPE — throwaway, not the real app.
// Run: cd prototype && swift run TandemPrototype
// Export surface PNGs to prototype/preview/: --export-previews
//
// Bikeshed map, ticket 02: the main window gains a second tab —
// Todos | Bikeshed — and this prototype explores the bikeshed surface:
// three search placements (tree / bar / panel) × two backlink placements
// (bottom dock / side rail), cycled from the floating bottom bar
// (←/→ cycle the search style). The editor pane is the live-preview mock
// (ticket 01 settled the real tech: CM6 in a WKWebView).

@main
struct TandemPrototypeApp: App {
    @State private var model = TandemModel()
    @State private var bike = BikeModel()
    // B (subtle) is the chosen reach (2026-10-11); A and C stay one tap
    // away in the switcher as references.
    @State private var reach: Reach = .subtle
    @State private var face: Face = .main
    // The prototype opens on the tab under judgment.
    @State private var tab: MainTab = .bikeshed
    @State private var mode: AppearanceMode = .system
    @State private var showBanner = false
    @State private var menuBar: MenuBarController?
    @State private var monitor: Any?

    init() {
        if CommandLine.arguments.contains("--export-previews") {
            PreviewExporter.run()
            exit(0)
        }
    }

    var body: some Scene {
        WindowGroup("Tandem — PROTOTYPE") {
            RootView(model: model,
                     bike: bike,
                     reach: $reach,
                     face: $face,
                     tab: $tab,
                     mode: $mode,
                     showBanner: $showBanner)
                .frame(minWidth: 860, idealWidth: 1060, minHeight: 560, idealHeight: 680)
                .onAppear {
                    if menuBar == nil {
                        menuBar = MenuBarController(model: model, appearanceMode: { mode })
                    }
                    if monitor == nil {
                        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                            // ←/→ cycle the search style, unless a text field
                            // is focused (ticket 02's primary axis).
                            if event.keyCode == 123 || event.keyCode == 124,
                               !(NSApp.keyWindow?.firstResponder is NSText) {
                                let delta = event.keyCode == 124 ? 1 : -1
                                let all = BikeSearchStyle.allCases
                                let i = (bike.searchStyle.rawValue + delta + all.count) % all.count
                                bike.searchStyle = all[i]
                                return nil
                            }
                            return event
                        }
                    }
                }
        }
        .windowResizability(.contentMinSize)
        .commands {
            // Prototype only: suppress the default ⌘N “New Window” so ⌘N
            // can focus quick-add (settled default UX, map 2026-10-11).
            CommandGroup(replacing: .newItem) { }
        }
    }
}

struct RootView: View {
    var model: TandemModel
    var bike: BikeModel
    @Binding var reach: Reach
    @Binding var face: Face
    @Binding var tab: MainTab
    @Binding var mode: AppearanceMode
    @Binding var showBanner: Bool
    @Environment(\.colorScheme) private var systemScheme

    private var pal: Palette {
        switch mode {
        case .light:  .day
        case .dark:   .night
        case .system: systemScheme == .dark ? .night : .day
        }
    }

    var body: some View {
        Group {
            switch face {
            case .main:
                MainFaceView(model: model, reach: reach, pal: pal, showBanner: showBanner,
                             bike: bike, tab: $tab)
            case .popover:
                ZStack {
                    pal.paper
                    PopoverMock(model: model, palette: pal, onOpenApp: {})
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .assets:
                AssetsFaceView(model: model)
            }
        }
        .overlay(alignment: .bottom) {
            SwitcherBar(model: model, bike: bike,
                        reach: $reach, face: $face, mode: $mode, showBanner: $showBanner,
                        onReset: { model.reset(); bike.reset() })
                .padding(.bottom, 10)
        }
        .preferredColorScheme(mode.colorScheme)
    }
}

// MARK: preview export (draft assets for the ticket)

@MainActor
enum PreviewExporter {
    static func run() {
        let dir = URL(fileURLWithPath: "preview", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let model = TandemModel()

        // 1 · bike illustration
        png(BikeScene(palette: .day, riders: true).background(Palette.day.paper),
            to: dir.appending(path: "bike-riders-day.png"), size: .init(width: 520, height: 340))
        png(BikeScene(palette: .night, riders: true).background(Palette.night.paper),
            to: dir.appending(path: "bike-riders-night.png"), size: .init(width: 520, height: 340))
        png(BikeScene(palette: .day, riders: false).background(Palette.day.paper),
            to: dir.appending(path: "bike-bare-day.png"), size: .init(width: 520, height: 340))
        png(BikeScene(palette: .night, riders: false).background(Palette.night.paper),
            to: dir.appending(path: "bike-bare-night.png"), size: .init(width: 520, height: 340))

        // 2 · app icon
        png(BikeIcon(palette: .day), to: dir.appending(path: "icon-day.png"), size: .init(width: 512, height: 512))
        png(BikeIcon(palette: .night), to: dir.appending(path: "icon-night.png"), size: .init(width: 512, height: 512))

        // 3 · menu bar glyph (mock strips)
        png(glyphStrip(bg: Color(white: 0.96), ink: Palette.day.ink),
            to: dir.appending(path: "glyph-day.png"), size: .init(width: 360, height: 88))
        png(glyphStrip(bg: Color(white: 0.09), ink: Palette.night.ink),
            to: dir.appending(path: "glyph-night.png"), size: .init(width: 360, height: 88))

        // 4 · wordmarks
        for (i, t) in WordmarkTreatment.allCases.enumerated() {
            png(Wordmark(treatment: t, palette: .day, height: 34).background(Palette.day.paper),
                to: dir.appending(path: "wordmark-\(i + 1)-day.png"), size: .init(width: 640, height: 180))
            png(Wordmark(treatment: t, palette: .night, height: 34).background(Palette.night.paper),
                to: dir.appending(path: "wordmark-\(i + 1)-night.png"), size: .init(width: 640, height: 180))
        }

        // 5 · reach variants, both faces (base app — todos tab)
        for r in Reach.allCases {
            for (label, pal) in [("day", Palette.day), ("night", Palette.night)] {
                let name = "reach-\(String(r.short.prefix(1)))-\(label).png"
                png(MainFaceView(model: model, reach: r, pal: pal, showBanner: r != .brandOnly,
                                 bike: BikeModel(), tab: .constant(.todos)),
                    to: dir.appending(path: name), size: .init(width: 900, height: 700),
                    scheme: label == "night" ? .dark : .light)
            }
        }

        // 6 · bikeshed surface — search style × backlink style, day + night
        //    (the ticket 02 question; the main window with the tab active)
        for s in BikeSearchStyle.allCases {
            for b in BikeBacklinkStyle.allCases {
                for (label, pal) in [("day", Palette.day), ("night", Palette.night)] {
                    let bike = BikeModel()
                    bike.searchStyle = s
                    bike.backlinkStyle = b
                    let name = "bikeshed-\(s.label)-\(b.label)-\(label).png"
                    png(MainFaceView(model: model, reach: .subtle, pal: pal, showBanner: false,
                                     bike: bike, tab: .constant(.bikeshed)),
                        to: dir.appending(path: name), size: .init(width: 1060, height: 680),
                        scheme: label == "night" ? .dark : .light)
                }
            }
        }
        for (label, pal) in [("day", Palette.day), ("night", Palette.night)] {
            png(PopoverMock(model: model, palette: pal, onOpenApp: {}),
                to: dir.appending(path: "popover-\(label).png"), size: .init(width: 400, height: 560),
                scheme: label == "night" ? .dark : .light)
        }

        // asset gallery (palette swatches included)
        png(AssetsFaceView(model: model), to: dir.appending(path: "assets.png"),
            size: .init(width: 1000, height: 2100))

        print("Previews written to preview/")
    }

    private static func glyphStrip(bg: Color, ink: Color) -> some View {
        HStack(spacing: 20) {
            BikeScene(palette: Palette(
                paper: .clear, parchment: .clear, sand: .clear, shadow: .clear,
                ink: ink, inkDim: ink, dustyBlue: ink, dustyBlueDeep: ink,
                ember: ink, emberDeep: ink, moss: ink, mossDeep: ink,
                sky: .clear, hill: .clear, hillFar: .clear, skin: ink, sun: ink),
                riders: false, lineWidth: 6)
                .frame(width: 64, height: 42)
            Text("template · system-tinted")
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(ink.opacity(0.6))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(bg)
    }

    private static func png<V: View>(_ view: V, to url: URL, size: CGSize, scale: CGFloat = 2,
                                     scheme: ColorScheme = .light) {
        // ImageRenderer ignores the app appearance; the environment's
        // colorScheme drives semantic colors and native controls, and the
        // export flag keeps offscreen TextFields from rendering as focused.
        let renderer = ImageRenderer(content: view.frame(width: size.width, height: size.height)
            .environment(\.colorScheme, scheme)
            .environment(\.tandemExport, true))
        renderer.scale = scale
        guard let nsImage = renderer.nsImage,
              let tiff = nsImage.tiffRepresentation,
              let rep = NSBitmapImageRep(data: tiff),
              let data = rep.representation(using: .png, properties: [:]) else { return }
        try? data.write(to: url)
    }
}
