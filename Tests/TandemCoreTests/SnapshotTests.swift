import AppKit
import Foundation
import SnapshotTesting
import SwiftUI
import Testing

@testable import Tandem
import TandemCore

// Snapshots: the four faces rendered and compared to committed baselines (TAN-218).
//
// **Why**: the gate can read a diff but cannot see that a column is 1100px wide or that
// a button lost its padding. These tests render the real views and fail when one pixel
// moves, naming the view that moved.
//
// **How they work**: each test seeds a throwaway `TANDEM_HOME` (never `~/.tandem`), hosts
// the real `BoardFace` / `TicketSheet` / `SessionsFaceView` / `SprintsFaceView` in an
// `NSHostingView` at a fixed size, and asserts two artefacts: the PNG (`.image(at:size:)`,
// the library's image snapshot pinned to a 2x render so the pixel size is the same on a
// Retina Mac and a headless runner) and the AppKit view tree as text
// (`.stableDescription`), whose frames read in a text diff without opening an image.
//
// **Why the text dump is normalised**: the raw `.recursiveDescription` carries two fields
// that are not layout, and CI's `xcode-27` runner disagreed with this Mac on both while
// every frame matched (TAN-218). Swift mangles an eight-hex local type discriminator into
// some class names, and that number is a property of the toolchain, so a Swift update
// re-diffs every line carrying one. The bracketed flag column records live AppKit state
// (`L` needsLayout, `U` needsUpdateConstraints, `W` wantsLayer), so it records whether a
// layout pass had settled on that machine. `.stableDescription` strips the discriminator
// and the flag column, and drops the legend that documents the flags; class names, tree
// shape, hidden state and every `f=` frame stay, so a layout change still moves the file.
//
// **What still differs across machines**: the ticket sheet's pop-up buttons are the one
// self-sizing AppKit control in the four views, and the runner lays them out a point or
// four wider than this Mac (it measures their menu labels from the machine). That view's
// text assertion passes `tolerance: 4` to `.stableDescription`, which still requires the
// same tree and the same class on every line and fails on any frame drift over four points;
// the other three views compare exactly. The PNGs compare perceptually at 99%, because the
// runner's rasteriser draws the AppKit controls differently even where the frames match.
// The text dump is the AppKit layer only: SwiftUI draws its own text into layers, not
// into `NSView`s, and the AX tree is unreadable from inside the test process (AX is
// disabled for it, `-25208`).
//
// **Re-recording on purpose** (a deliberate layout change, once you have looked at
// the new pictures and they are right):
//
//     SNAPSHOT_TESTING_RECORD=all swift test --filter SnapshotTests
//
// then read every changed file under `Tests/TandemCoreTests/__Snapshots__/` before
// committing it. The PNG *is* what the change looks like. A test with no baseline
// fails and records the file it will compare against next time, so a missing baseline
// is red once by design.
//
// **What a baseline is tied to**: this machine's macOS and fonts (the library's own
// note: snapshots are compared on the OS that recorded them). The CI runner is the
// same macOS 27 image the app is built for.

@Suite("View snapshots", .serialized)
@MainActor
struct SnapshotTests {

    // MARK: - The fixture home

    /// The throwaway home every snapshot renders against, built once. `TANDEM_HOME` is
    /// set before the app's models are first touched, so `TandemModel.shared` and
    /// `BikeshedModel.shared` resolve into it: the faces draw the fixture, and no
    /// test can reach the human's home.
    @MainActor
    enum Fixture {
        static let home: URL = {
            let home = FileManager.default.temporaryDirectory
                .appendingPathComponent("tandem-snapshots-\(UUID().uuidString)")
            try! FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
            setenv("TANDEM_HOME", home.path, 1)
            try! seedStore(at: home)
            try! seedSessions(at: home)
            try! seedSprints(at: home)
            return home
        }()

        static let model: TandemModel = {
            _ = home
            // The app follows the system; a snapshot must not, so it renders the day
            // panel whatever this machine is set to.
            NSApplication.shared.appearance = NSAppearance(named: .aqua)
            return TandemModel.shared
        }()

        static let bikeModel: BikeshedModel = {
            _ = home
            return BikeshedModel.shared
        }()

        // MARK: The data

        /// Two lanes, five tickets, every workflow stage the board draws a column for.
        private static func seedStore(at home: URL) throws {
            let store = Store(directory: home)
            _ = try store.addLane("Tandem")
            _ = try store.addLane("Fleet")
            _ = try store.add("Land the snapshot seam", lane: "Tandem",
                              status: .inProgress, assignee: "builder",
                              description: "Render the four faces and compare them to "
                                  + "committed baselines, so a layout change a diff "
                                  + "cannot show fails swift test.")
            _ = try store.add("Refine the driver question", lane: "Tandem",
                              status: .backlog, assignee: "human")
            _ = try store.add("Widen the editor column", lane: "Tandem",
                              status: .inReview, assignee: "reviewer")
            _ = try store.add("Sweep the open tickets", lane: "Fleet",
                              status: .done, assignee: "builder")
            _ = try store.add("Rename the cask token", status: .backlog)
        }

        /// Three records: one still running, two ended with their outcomes.
        private static func seedSessions(at home: URL) throws {
            let records = [
                Session(id: "ses_fixture_a", project: "tandem", agent: "builder",
                        brief: "Build TAN-218.", status: .running,
                        startedAt: Date(timeIntervalSince1970: 1_793_264_400)),
                Session(id: "ses_fixture_b", project: "tandem", agent: "reviewer",
                        brief: "Review TAN-212.", summary: "Merged.",
                        status: .ended, outcome: .succeeded,
                        startedAt: Date(timeIntervalSince1970: 1_793_178_000),
                        endedAt: Date(timeIntervalSince1970: 1_793_181_600)),
                Session(id: "ses_fixture_c", project: "fleet", agent: nil,
                        brief: "Tidy the roster.", status: .ended, outcome: .interrupted,
                        startedAt: Date(timeIntervalSince1970: 1_793_006_400),
                        endedAt: Date(timeIntervalSince1970: 1_793_008_000)),
            ]
            try write(SessionsFile(version: 1, sessions: records),
                      to: home.appendingPathComponent("sessions.json"))
        }

        /// Two lanes' records: one open, one closed.
        private static func seedSprints(at home: URL) throws {
            let tickets = try Store(directory: home).list(.all).value.map(\.id)
            let records = [
                Sprint(lane: "Tandem", number: 4, mode: .parallel,
                       batch: Array(tickets.prefix(2)),
                       runs: ["2026-10-29T09:00:00Z — Opened."],
                       status: .open,
                       openedAt: Date(timeIntervalSince1970: 1_793_264_400)),
                Sprint(lane: "Fleet", number: 2, mode: .serial,
                       runs: ["2026-10-27T10:00:00Z — Opened."],
                       close: "Landed the roster notes.", status: .closed,
                       openedAt: Date(timeIntervalSince1970: 1_793_092_800),
                       closedAt: Date(timeIntervalSince1970: 1_793_100_000)),
            ]
            try write(SprintsFile(version: 1, sprints: records),
                      to: home.appendingPathComponent("sprints.json"))
        }

        /// The envelope both legacy files carry, so the fixture is the shape the core
        /// reads (and imports) rather than a shape invented here.
        private struct SessionsFile: Encodable {
            var version: Int
            var sessions: [Session]
        }

        private struct SprintsFile: Encodable {
            var version: Int
            var sprints: [Sprint]
        }

        private static func write<T: Encodable>(_ value: T, to url: URL) throws {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            try encoder.encode(value).write(to: url)
        }
    }

    // MARK: - The views

    /// The board: two lanes, every stage column, a search field and the archive button.
    @Test("the board face")
    func boardFace() throws {
        let focus = FocusState<Bool>()
        let view = BoardFace(model: Fixture.model,
                             search: .constant(""),
                             palette: .day,
                             focus: focus.projectedValue,
                             onOpenLink: { _ in },
                             onPromote: { _ in .refused(message: "fixture") })
        let size = CGSize(width: 1000, height: 640)
        let hosting = host(view, size: size)
        assertSnapshot(of: hosting, as: .image(at: 2, size: size))
        assertSnapshot(of: hosting, as: .stableDescription())
    }

    /// The ticket sheet: title, the three pickers, the filer, the ref, the body.
    @Test("the ticket sheet")
    func ticketSheet() throws {
        let ticket = try #require(Fixture.model.tickets.first {
            $0.status == .inProgress
        })
        let view = TicketSheet(ticket: ticket,
                               palette: .day,
                               lanes: Fixture.model.lanes,
                               roster: TandemSettings.defaultRoster,
                               onSave: { _ in },
                               onCancel: {},
                               onComment: { _ in .refused(message: "fixture") },
                               onRefreshComments: { .list([]) },
                               onOpenLink: { _ in },
                               onPromote: { .refused(message: "fixture") })
        // The sheet sizes itself (580 wide, 560 tall with no comments to draw).
        let size = CGSize(width: 580, height: 560)
        let hosting = host(view, size: size)
        assertSnapshot(of: hosting, as: .image(at: 2, size: size))
        assertSnapshot(of: hosting, as: .stableDescription(tolerance: 4))
    }

    /// The sessions face: two projects, a running record and two ended ones.
    @Test("the sessions face")
    func sessionsFace() throws {
        let view = SessionsFaceView(model: Fixture.model,
                                    bikeModel: Fixture.bikeModel,
                                    palette: .day,
                                    onOpenLink: { _ in })
        let size = CGSize(width: 900, height: 520)
        let hosting = host(view, size: size)
        assertSnapshot(of: hosting, as: .image(at: 2, size: size))
        assertSnapshot(of: hosting, as: .stableDescription())
    }

    /// The sprints face: an open sprint and a closed one, grouped by lane.
    @Test("the sprints face")
    func sprintsFace() throws {
        let view = SprintsFaceView(model: Fixture.model,
                                   bikeModel: Fixture.bikeModel,
                                   palette: .day,
                                   onOpenLink: { _ in })
        let size = CGSize(width: 900, height: 520)
        let hosting = host(view, size: size)
        assertSnapshot(of: hosting, as: .image(at: 2, size: size))
        assertSnapshot(of: hosting, as: .stableDescription())
    }

    // MARK: - The normalised text dump

    @Test("the sheet's text diff allows four points of AppKit drift and no more")
    func geometryTolerance() {
        let reference = "  h=--- v=--- SwiftUI.AppKitTextField f=(20,65,58,13) b=(-)"
        let diffing = Diffing<String>.geometry(tolerance: 4)
        #expect(diffing.diffV2(reference, reference) == nil)
        #expect(diffing.diffV2(reference,
                               "  h=--- v=--- SwiftUI.AppKitTextField f=(20,65,62,13) b=(-)") == nil)
        #expect(diffing.diffV2(reference,
                               "  h=--- v=--- SwiftUI.AppKitTextField f=(24,65,58,13) b=(-)") == nil)
        #expect(diffing.diffV2(reference,
                               "  h=--- v=--- SwiftUI.AppKitTextField f=(20,65,63,13) b=(-)") != nil)
        #expect(diffing.diffV2(reference,
                               "  h=--- v=--- SwiftUI.AppKitTextField f=(24,65,58,13) b=(-) !") != nil)
        #expect(diffing.diffV2(reference,
                               "  h=--- v=--- SwiftUI.AppKitTextEditor f=(20,65,58,13) b=(-)") != nil)
    }

    /// The two machine-dependent fields, removed. The eight-hex discriminator follows a
    /// `$` inside some mangled class names; the flag column is the first bracketed field on
    /// a line, and the legend that documents it is the trailing paragraph.
    @Test("the text dump keeps frames and drops the toolchain discriminator and the live flags")
    func stableTextDump() {
        let raw = [
            #"[   AF    ! WlU ] h=--- v=--- _TtGC7SwiftUI22AppKitPlatformViewHostGVS_P10$1ba81981832PlatformViewRepresentableAdaptorVS_P10$1ba863fec24PlatformTextFieldAdaptor__ f=(51,21,817,15) b=(-) => <NSViewBackingLayer>"#,
            #"  [  HAF    ! W U ] h=--- v=--- NSScroller f=(0,0,0,100) b=(-) => <NSViewBackingLayer>"#,
            "A=autoresizesSubviews, C=canDrawConcurrently, D=needsDisplay",
        ].joined(separator: "\n")

        let stable = Snapshotting<NSView, String>.stable(raw)

        #expect(!stable.contains("$1ba81981832"))
        #expect(!stable.contains("WlU"))
        #expect(!stable.contains("A=autoresizesSubviews"))
        #expect(stable.contains("f=(51,21,817,15)"))
        #expect(stable.contains("f=(0,0,0,100)"))
        #expect(stable.contains("NSScroller"))
        #expect(stable.contains("h=--- v=---"))
    }

    // MARK: - Hosting

    private func host<V: View>(_ view: V, size: CGSize) -> NSView {
        let hosting = NSHostingView(rootView: view.frame(width: size.width, height: size.height))
        hosting.frame = NSRect(origin: .zero, size: size)
        hosting.layoutSubtreeIfNeeded()
        return hosting
    }
}


// MARK: - A stable text dump

extension Snapshotting where Value == NSView, Format == String {
    /// The AppKit tree with the fields that are not layout removed, so the same layout
    /// records the same text on this Mac and on CI's runner. See the note at the top.
    static func stableDescription(tolerance: CGFloat = 0) -> Snapshotting<NSView, String> {
        let base = Snapshotting<NSView, String>.recursiveDescription
        return Snapshotting(
            pathExtension: base.pathExtension,
            diffing: tolerance > 0 ? Diffing<String>.geometry(tolerance: tolerance) : base.diffing,
            asyncSnapshot: { view in base.snapshot(view).map(stable) }
        )
    }

    static func stable(_ description: String) -> String {
        description
            .split(separator: "\n", omittingEmptySubsequences: false)
            .filter { !$0.hasPrefix("A=autoresizesSubviews") }
            .map { line -> String in
                var line = line.replacingOccurrences(
                    of: #"\$[0-9a-fA-F]{8}"#, with: "", options: .regularExpression)
                let indent = String(line.prefix { $0 == " " })
                let rest = line.dropFirst(indent.count)
                if rest.first == "[", let close = rest.firstIndex(of: "]") {
                    var tail = rest[rest.index(after: close)...]
                    if tail.first == " " { tail = tail.dropFirst() }
                    return indent + tail
                }
                return line
            }
            .joined(separator: "\n")
    }
}


// MARK: - A text diff that lets the geometry drift

extension Diffing where Value == String {
    /// Compares two dumps line by line: everything except the `f=`/`b=` geometry must match
    /// exactly, and each coordinate may drift by `tolerance` points. The reference for the
    /// ticket sheet is recorded on this Mac, and CI's runner lays its pop-up buttons out a
    /// point or four wider (AppKit measures the menu labels from the machine, and the sheet is
    /// the one face carrying self-sizing controls), so that view's assertion is the only one
    /// that needs room. Every other face compares with the plain `.lines` diffing.
    static func geometry(tolerance: CGFloat) -> Diffing<String> {
        .diff(
            toData: { Data($0.utf8) },
            fromData: { String(decoding: $0, as: UTF8.self) },
            diffV2: { reference, recorded in
                let expected = reference.split(separator: "\n", omittingEmptySubsequences: false)
                let actual = recorded.split(separator: "\n", omittingEmptySubsequences: false)
                guard expected.count == actual.count else {
                    return (
                        "The dump has \(actual.count) lines; the reference has \(expected.count).",
                        []
                    )
                }
                for (index, pair) in zip(expected, actual).enumerated() {
                    let left = String(pair.0)
                    let right = String(pair.1)
                    let line = index + 1
                    guard skeleton(left) == skeleton(right) else {
                        return ("Line \(line) changed:\n\n- \(left)\n+ \(right)", [])
                    }
                    if let drift = geometryDrift(left, right, tolerance: tolerance) {
                        return (
                            "Line \(line) moved by \(drift) points (tolerance \(tolerance)):"
                                + "\n\n- \(left)\n+ \(right)",
                            []
                        )
                    }
                }
                return nil
            }
        )
    }
}

/// The line with its geometry replaced by a fixed token, so a frame move does not read as a
/// change to the class or the hidden state.
private func skeleton(_ line: String) -> String {
    line.replacing(/[fb]=\([^)]*\)/, with: "GEOM")
}

/// The first coordinate pair that drifted further than `tolerance`, or `nil` when every
/// number still matches within it. A `-` placeholder in `b=` must match a `-` exactly.
private func geometryDrift(_ left: String, _ right: String, tolerance: CGFloat) -> CGFloat? {
    let expected = left.matches(of: /[fb]=\(([^)]*)\)/).map { String($0.output.1) }
    let actual = right.matches(of: /[fb]=\(([^)]*)\)/).map { String($0.output.1) }
    guard expected.count == actual.count else { return .infinity }
    for (a, b) in zip(expected, actual) {
        let expectedParts = a.split(separator: ",")
        let actualParts = b.split(separator: ",")
        guard expectedParts.count == actualParts.count else { return .infinity }
        for (x, y) in zip(expectedParts, actualParts) {
            guard let expectedValue = Double(x), let actualValue = Double(y) else {
                if x != y { return .infinity }
                continue
            }
            let drift = abs(expectedValue - actualValue)
            if drift > Double(tolerance) { return CGFloat(drift) }
        }
    }
    return nil
}


// MARK: - A 2x image snapshot

extension Snapshotting where Value == NSView, Format == NSImage {
    /// The library's `.image(size:)` takes the view's own backing store, and an offscreen
    /// view not in a window borrows the screen's scale factor (2 on a Retina Mac, 1 on a
    /// headless runner), which would make the committed PNG's pixel size a property of the
    /// machine. This renders into a bitmap of a named scale instead, so the baseline is
    /// the same file everywhere, and still sharp.
    ///
    /// The comparison is perceptual, not exact (TAN-218). The baseline is recorded on this
    /// Mac, and CI's runner rasterises the AppKit controls and measures their labels a point
    /// or four differently, so three of the four PNGs diffed on every run while the board's
    /// matched byte for byte. 99% of pixels must match within 99% of the eye's precision,
    /// the library's own note for cross-machine images; the tests' geometry check is the
    /// normalised text dump, and the sheet's frames carry their own tolerance above, so a
    /// layout move still fails even though a glyph edge may not.
    @MainActor
    static func image(at scale: Int, size: CGSize) -> Snapshotting<NSView, NSImage> {
        Snapshotting<NSImage, NSImage>.image(precision: 0.99, perceptualPrecision: 0.99)
            .pullback { view in
            let initial = view.frame.size
            view.frame.size = size
            view.layoutSubtreeIfNeeded()
            let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil,
                                          pixelsWide: Int(size.width) * scale,
                                          pixelsHigh: Int(size.height) * scale,
                                          bitsPerSample: 8,
                                          samplesPerPixel: 4,
                                          hasAlpha: true,
                                          isPlanar: false,
                                          colorSpaceName: .deviceRGB,
                                          bytesPerRow: 0,
                                          bitsPerPixel: 0)!
            bitmap.size = size
            view.cacheDisplay(in: view.bounds, to: bitmap)
            view.frame.size = initial
            // Through PNG data: one Sendable value, and the file compared is the file
            // the diffing writes.
            let data = bitmap.representation(using: .png, properties: [:])!
            return NSImage(data: data)!
        }
    }
}
