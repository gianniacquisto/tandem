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

        }

    // MARK: - The junk gate (ticket 824CB789)

    @Test("capture refuses an empty or whitespace-only body, filing nothing")
    func captureRefusesEmptyBody() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        for body in ["", "   ", "\n\n", " \t\n "] {
            #expect(throws: BikeshedError.emptyCaptureBody) {
                _ = try bikeshed.capture(
                    project: "tandem", kind: .work, body: body, session: "s", at: instant)
            }
        }
        #expect(!FileManager.default.fileExists(atPath: dir.path),
            "a refused capture must not create the bikeshed")
    }

    @Test("capture refuses a body whose slug is empty — the kind-name fallback was the junk")
    func captureRefusesSluglessBody() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        for body in ["!!!", "# !!!", "---", "###"] {
            #expect(throws: BikeshedError.emptyCaptureSlug(kind: "work")) {
                _ = try bikeshed.capture(
                    project: "tandem", kind: .work, body: body, session: "s", at: instant)
            }
        }
        #expect(!FileManager.default.fileExists(atPath: dir.path))
    }

    @Test("capture files a body whose first line names it — the `# x` shape survives")
    func captureAcceptsTitleBody() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)

        let path = try bikeshed.capture(
            project: "tandem", kind: .work, body: "# x\n\nbody", session: "s", at: instant).value
        #expect(path == "projects/tandem/.raw/s/\(instantStamp)-work-x.md")
    }

    @Test("capture dedupes: same project, kind, and body re-file nothing")
    func captureDeduplicates() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        let body = "# Zanzibar\n\nthe body"

        let first = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: body, session: "ses_a", at: instant)
        let second = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: body, session: "ses_b",
            at: instant.addingTimeInterval(60))

        #expect(second.value == first.value, "the re-file names the existing capture")
        #expect(second.note != nil, "the re-file says why nothing was written")
        let files = filesUnderRaw(of: dir, project: "tandem")
        #expect(files.count == 1, "no second file was written")
    }

    @Test("dedupe identity: kind, body, and project must all match")
    func dedupeIdentity() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        let body = "# Zanzibar"

        _ = try bikeshed.capture(project: "tandem", kind: .gotcha, body: body, session: "s", at: instant)

        // A different kind files its own note; a different body files its own;
        // the same body under another project files its own.
        let otherKind = try bikeshed.capture(
            project: "tandem", kind: .fact, body: body, session: "s", at: instant)
        let otherBody = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: "# Other", session: "s", at: instant)
        let otherProject = try bikeshed.capture(
            project: "other", kind: .gotcha, body: body, session: "s", at: instant)

        #expect(otherKind.note == nil)
        #expect(otherBody.note == nil)
        #expect(otherProject.note == nil)
        #expect(filesUnderRaw(of: dir, project: "tandem").count == 3,
            "the original, the other kind, and the other body each filed one")
        #expect(filesUnderRaw(of: dir, project: "other").count == 1)
    }

    @Test("dedupe ignores the archive: a consumed capture can be filed again")
    func dedupeIgnoresArchive() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        let body = "# Zanzibar"

        // An archived (consumed) copy of the same capture, the shape the
        // consolidation pass leaves behind under `.raw/.done/<session>/`.
        let done = dir.appendingPathComponent("projects/tandem/.raw/.done/ses_old")
        try FileManager.default.createDirectory(at: done, withIntermediateDirectories: true)
        try Data(body.utf8).write(to: done.appendingPathComponent("\(instantStamp)-gotcha-zanzibar.md"))

        let filed = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: body, session: "ses_new", at: instant)

        #expect(filed.note == nil, "the archive is out of the pending scan")
        #expect(filed.value.hasPrefix("projects/tandem/.raw/ses_new/"))
    }

    // MARK: - Helpers (junk gate)

    /// The pending capture files of one project, recursively — the set the
    /// duplicate scan reads.
    private func filesUnderRaw(of dir: URL, project: String) -> [String] {
        let raw = dir.appendingPathComponent("projects/\(project)/.raw")
        let fm = FileManager.default
        guard let sessions = try? fm.contentsOfDirectory(at: raw, includingPropertiesForKeys: nil) else {
            return []
        }
        var found: [String] = []
        for session in sessions where !session.lastPathComponent.hasPrefix(".") {
            let files = (try? fm.contentsOfDirectory(at: session, includingPropertiesForKeys: nil)) ?? []
            found.append(contentsOf: files.map(\.lastPathComponent))
        }
        return found
    }

    @Test("capture suffixes a same-second, same-slug collision and keeps both notes")
    func captureCollisionSuffix() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir.deletingLastPathComponent()) }
        let bikeshed = Bikeshed(directory: dir)
        // Same slug, different bytes: the collision the suffix exists for —
        // a byte-identical body is the duplicate no-op instead (the junk gate).
        let body = "# Caret not placed"
        let twin = "# caret not placed"

        let first = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: body, session: "ses", at: instant).value
        let second = try bikeshed.capture(
            project: "tandem", kind: .gotcha, body: twin, session: "ses", at: instant).value

        #expect(first == "projects/tandem/.raw/ses/\(instantStamp)-gotcha-caret-not-placed.md")
        #expect(second == "projects/tandem/.raw/ses/\(instantStamp)-gotcha-caret-not-placed-2.md")

        // Nothing was overwritten: both files are on disk, each with its body.
        for (path, text) in [(first, body), (second, twin)] {
            let onDisk = try String(contentsOf: dir.appendingPathComponent(path), encoding: .utf8)
            #expect(onDisk == text)
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
