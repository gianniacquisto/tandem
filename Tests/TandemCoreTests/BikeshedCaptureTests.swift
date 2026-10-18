import Foundation
import Testing
@testable import TandemCore

/// `note capture` at the core seam (ADR 0016, raw-layer design): one raw memory
/// note per moment, written to the hidden per-project capture layer
/// `projects/<project>/.raw/<session>/<utc>-<kind>-<slug>.md`. Covers the naming
/// rule (path, slug, UTC stamp), collision suffixes, project/session validation,
/// and the property the layer exists for — a captured note is *out of the
/// curated graph*: absent from `list` / `notePaths`, unmatched by `search`, and
/// unresolvable.
///
/// Every test runs against a temp dir via `Bikeshed`'s explicit-directory
/// initialiser — no fixture bikeshed, no real `~/.tandem`, no network.
@Suite("Bikeshed capture")
struct BikeshedCaptureTests {

    /// A unique temp directory (not pre-created: a missing bikeshed folder is a
    /// valid empty bikeshed). Nested one level below a unique parent so the
    /// bikeshed's sibling lockfile (`bikeshed.lock`, ADR 0004) is never shared
    /// between concurrent tests. Callers clean up with `defer`.
    private func tempDir() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-capture-\(UUID().uuidString)")
            .appendingPathComponent("bikeshed")
    }

    /// A fixed instant, so the `<utc>` filename stamp is a literal the test
    /// spells out (2026-10-18T14:02:11Z) instead of recomputing it the way the
    /// code does.
    private let instant = Date(timeIntervalSince1970: 1_792_332_131)
    private let instantStamp = "20261018T140211Z"

    /// The curated `list` paths, recursively — the graph a capture must stay out of.
    private func listPaths(_ bikeshed: Bikeshed) throws -> [String] {
        var paths: [String] = []
        func walk(_ nodes: [BikeshedNode]) {
            for node in nodes {
                paths.append(node.path)
                walk(node.children)
            }
        }
        walk(try bikeshed.list().value)
        return paths
    }

    // MARK: - The write

    @Test("capture writes the body verbatim to the hidden path and returns it")
    func captureWritesBodyVerbatim() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let body = """
        # Caret not placed

        The editor opened without focus; a click was needed before typing.
        """
        let result = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: body, session: "ses_test", at: instant)

        let expected = "projects/tandem/.raw/ses_test/\(instantStamp)-gotcha-caret-not-placed.md"
        #expect(result.value == expected)
        #expect(result.note == nil)

        // Byte-for-byte, straight off disk, bypassing the core.
        let onDisk = try String(contentsOf: dir.appendingPathComponent(expected), encoding: .utf8)
        #expect(onDisk == body, "the core stores exactly the bytes given")
    }

    @Test("capture creates the parent folders (project, .raw, session)")
    func captureCreatesParents() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        _ = try bikeshed.capture(
            project: "tandem", kind: .fact, body: "a fact", session: "ses_x", at: instant)

        var isDir: ObjCBool = false
        let sessionFolder = dir.appendingPathComponent("projects/tandem/.raw/ses_x")
        #expect(FileManager.default.fileExists(atPath: sessionFolder.path, isDirectory: &isDir)
            && isDir.boolValue)
    }

    @Test("capture slug: leading # stripped, non-alphanumerics collapsed, 40-char cap")
    func captureSlugRule() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        // Leading # / whitespace stripped; the em dash, apostrophe, and the
        // doubled spaces collapse to single dashes.
        let body = "\n\n   ### Gotcha — the caret wasn't placed on open!!\nmore"
        let path = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: body, session: "s", at: instant).value
        #expect(path == "projects/tandem/.raw/s/\(instantStamp)-gotcha-gotcha-the-caret-wasn-t-placed-on-open.md")

        // The slug is capped at 40 characters.
        let long = "# " + String(repeating: "a", count: 45)
        let capped = try bikeshed.capture(
            project: "tandem", kind: .fact, body: long, session: "s", at: instant).value
        let expectedCapped = "projects/tandem/.raw/s/\(instantStamp)-fact-\(String(repeating: "a", count: 40)).md"
        #expect(capped == expectedCapped)

        // An empty body (no slug-able line) falls back to the kind.
        let empty = try bikeshed.capture(
            project: "tandem", kind: .work, body: "", session: "s", at: instant).value
        #expect(empty == "projects/tandem/.raw/s/\(instantStamp)-work-work.md")
    }

    @Test("capture suffixes a same-second, same-slug collision and keeps both notes")
    func captureCollisionSuffix() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        let body = "# Caret not placed"

        let first = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: body, session: "ses", at: instant).value
        let second = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: body, session: "ses", at: instant).value
        let third = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: body, session: "ses", at: instant).value

        #expect(first == "projects/tandem/.raw/ses/\(instantStamp)-gotcha-caret-not-placed.md")
        #expect(second == "projects/tandem/.raw/ses/\(instantStamp)-gotcha-caret-not-placed-2.md")
        #expect(third == "projects/tandem/.raw/ses/\(instantStamp)-gotcha-caret-not-placed-3.md")

        // Nothing was overwritten: all three files are on disk, each with the body.
        for path in [first, second, third] {
            let onDisk = try String(contentsOf: dir.appendingPathComponent(path), encoding: .utf8)
            #expect(onDisk == body)
        }
    }

    // MARK: - Validation

    @Test("capture rejects a project or session that is not one visible folder name")
    func captureRejectsBadComponents() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        for bad in ["", ".", "..", ".hidden", "a/b", "a#b", "a|b"] {
            #expect(throws: BikeshedError.invalidCaptureName(name: bad)) {
                _ = try bikeshed.capture(
                    project: bad, kind: .fact, body: "x", session: "ses", at: instant)
            }
            #expect(throws: BikeshedError.invalidCaptureName(name: bad)) {
                _ = try bikeshed.capture(
                    project: "tandem", kind: .fact, body: "x", session: bad, at: instant)
            }
        }

        // Validation throws before the lock, so nothing was created at all.
        #expect(!FileManager.default.fileExists(atPath: dir.path),
            "a rejected capture must not create the bikeshed")
    }

    // MARK: - Out of the graph

    @Test("a captured note is absent from list and notePaths")
    func captureIsNotListed() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        _ = try bikeshed.write(path: "Real.md", text: "a curated note")

        let path = try bikeshed.capture(
            project: "tandem", kind: .fact, body: "# Zanzibar\n", session: "ses", at: instant).value

        let listed = try listPaths(bikeshed)
        #expect(!listed.contains(path))
        #expect(!listed.contains { $0.contains(".raw") }, "the whole .raw subtree is pruned")
        #expect(!bikeshed.notePaths().contains(path))

        // The curated note is still there, next to the hidden folder.
        #expect(listed.contains("Real.md"))
        #expect(listed.contains("projects"))
        #expect(listed.contains("projects/tandem"))
    }

    @Test("a captured note is not matched by search and does not resolve")
    func captureIsNotSearchableOrResolvable() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let path = try bikeshed.capture(
            project: "tandem", kind: .fact, body: "# Zanzibar unicorn\n", session: "ses", at: instant).value

        // Not searched: the unique body text finds nothing in the curated graph.
        #expect(try bikeshed.search(query: "Zanzibar").value.isEmpty)

        // Not resolvable: a target equal to the capture's own basename would
        // resolve if the note were in the graph — it is hidden, so it is broken.
        var stem = (path as NSString).lastPathComponent
        if stem.lowercased().hasSuffix(".md") { stem.removeLast(3) }
        #expect(bikeshed.resolve(stem) == .broken)
        #expect(bikeshed.notePaths().allSatisfy { ($0 as NSString).lastPathComponent != stem + ".md" })
    }
}
