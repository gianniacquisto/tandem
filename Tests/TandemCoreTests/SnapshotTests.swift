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
        assertSnapshot(of: hosting, as: .stableDescription)
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
        assertSnapshot(of: hosting, as: .stableDescription)
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
        assertSnapshot(of: hosting, as: .stableDescription)
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
        assertSnapshot(of: hosting, as: .stableDescription)
    }

    // MARK: - The normalised text dump

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
    static var stableDescription: Snapshotting<NSView, String> {
        let base = Snapshotting<NSView, String>.recursiveDescription
        return Snapshotting(
            pathExtension: base.pathExtension,
            diffing: base.diffing,
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


// MARK: - A 2x image snapshot

extension Snapshotting where Value == NSView, Format == NSImage {
    /// The library's `.image(size:)` takes the view's own backing store, and an offscreen
    /// view not in a window borrows the screen's scale factor (2 on a Retina Mac, 1 on a
    /// headless runner), which would make the committed PNG's pixel size a property of the
    /// machine. This renders into a bitmap of a named scale instead, so the baseline is
    /// the same file everywhere, and still sharp.
    @MainActor
    static func image(at scale: Int, size: CGSize) -> Snapshotting<NSView, NSImage> {
        Snapshotting<NSImage, NSImage>.image.pullback { view in
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
