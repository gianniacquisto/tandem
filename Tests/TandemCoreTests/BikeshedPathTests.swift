import Foundation
import Testing
@testable import TandemCore

/// The bikeshed path-hygiene rule (ADR 0008): the one place that decides whether a
/// bikeshed-root-relative path is legal or a bikeshed escape. Previously three copies
/// (core, GUI, model) that had already diverged.
@Suite("Bikeshed path hygiene")
struct BikeshedPathTests {

    private let root = URL(fileURLWithPath: "/tmp/tandem-bikeshed")

    @Test("legal bikeshed-relative paths resolve under the root")
    func legalPaths() {
        #expect(BikeshedPath.url(under: root, for: "Note.md")?.path == "/tmp/tandem-bikeshed/Note.md")
        #expect(BikeshedPath.url(under: root, for: "a/b/C.md")?.path == "/tmp/tandem-bikeshed/a/b/C.md")
        // Spaces are legal — the create/rename rule allows them, and the
        // hygiene rule must not reject what the app itself writes.
        #expect(BikeshedPath.url(under: root, for: "My Note.md")?.path
            == "/tmp/tandem-bikeshed/My Note.md")
        // `#` and `|` are tolerated here (pre-existing on-disk violators the app
        // must still show and edit); `validateNewName` is the stricter rule.
        #expect(BikeshedPath.url(under: root, for: "a#b.md") != nil)
    }

    @Test("an empty path, empty components, and `.` / `..` escapes are refused")
    func refusedPaths() {
        #expect(BikeshedPath.url(under: root, for: "") == nil)
        // A leading or trailing slash makes an empty component.
        #expect(BikeshedPath.url(under: root, for: "/Note.md") == nil)
        #expect(BikeshedPath.url(under: root, for: "a//b.md") == nil)
        #expect(BikeshedPath.url(under: root, for: "a/") == nil)
        // Dot components escape or collapse.
        #expect(BikeshedPath.url(under: root, for: ".") == nil)
        #expect(BikeshedPath.url(under: root, for: "..") == nil)
        #expect(BikeshedPath.url(under: root, for: "../outside.md") == nil)
        #expect(BikeshedPath.url(under: root, for: "a/../../outside.md") == nil)
        #expect(BikeshedPath.url(under: root, for: "a/./b.md") == nil)
    }

    @Test("the resolved path is standardized regardless of the root's shape")
    func standardizedRoot() {
        // A trailing slash, or a `.` in the root, must not leak into the result
        // (the GUI copy standardized the root; the core copy did not — the rule
        // is unified on standardizing).
        let messy = URL(fileURLWithPath: "/tmp/tandem-bikeshed/./")
        #expect(BikeshedPath.url(under: messy, for: "a/b.md")?.path
            == "/tmp/tandem-bikeshed/a/b.md")
    }

    @Test("a component containing a slash cannot smuggle a path")
    func noSmuggle() {
        // There is no way to pass a component with an embedded slash: the split
        // makes it two components, both checked.
        #expect(BikeshedPath.url(under: root, for: "a/../b.md") == nil)
    }
}
