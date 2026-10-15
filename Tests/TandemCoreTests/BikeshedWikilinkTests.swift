import Foundation
import Testing
@testable import TandemCore

/// Wikilink model (spec: "Wikilink model", ADR 0005): grammar parsing
/// (bare, qualified, alias, fragment; first-`#`-then-first-`|` split order;
/// empty, same-note form), case-insensitive resolution with the exactly-one
/// rule, and the single resolver the backlinks and rename-rewrite paths
/// share.
@Suite("Bikeshed wikilink resolution")
struct BikeshedWikilinkTests {

    private func tempDir() -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-bikeshed-wikilink-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func stage(_ relativePath: String, _ text: String, in dir: URL) throws {
        let url = dir.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try text.write(to: url, atomically: false, encoding: .utf8)
    }

    // MARK: - Grammar

    @Test("parse: bare target, display is the target as written")
    func parseBare() throws {
        let link = Wikilink(raw: "Note")
        #expect(link?.target == "Note")
        #expect(link?.fragment == nil)
        #expect(link?.alias == nil)
        #expect(link?.displayText == "Note")
    }

    @Test("parse: alias — display is the alias")
    func parseAlias() throws {
        let link = Wikilink(raw: "Note|Alias")
        #expect(link?.target == "Note")
        #expect(link?.alias == "Alias")
        #expect(link?.displayText == "Alias")
    }

    @Test("parse: fragment — display is the fragment (inert in v1)")
    func parseFragment() throws {
        let link = Wikilink(raw: "Note#Heading")
        #expect(link?.target == "Note")
        #expect(link?.fragment == "Heading")
        #expect(link?.alias == nil)
        #expect(link?.displayText == "Heading")
    }

    @Test("parse: fragment + alias — split at first #, then first |")
    func parseFragmentAndAlias() throws {
        let link = Wikilink(raw: "Note#Heading|Alias")
        #expect(link?.target == "Note")
        #expect(link?.fragment == "Heading")
        #expect(link?.alias == "Alias")
        #expect(link?.displayText == "Alias")
    }

    @Test("parse: later # and | stay in the fragment/alias text")
    func parseFirstHashThenFirstPipe() throws {
        let link = Wikilink(raw: "a#b|c#d")
        #expect(link?.target == "a")
        #expect(link?.fragment == "b")
        #expect(link?.alias == "c#d")
        #expect(link?.displayText == "c#d")
    }

    @Test("parse: alias before fragment is still parsed (reversed order)")
    func parseReversedOrder() throws {
        let link = Wikilink(raw: "a|b#Heading")
        #expect(link?.target == "a")
        #expect(link?.alias == "b")
        #expect(link?.fragment == "Heading")
        #expect(link?.displayText == "b")   // the alias, when present
    }

    @Test("parse: empty or whitespace-only target is not a link")
    func parseEmptyTarget() throws {
        #expect(Wikilink(raw: "") == nil)
        #expect(Wikilink(raw: "   ") == nil)
        #expect(Wikilink(raw: "\t") == nil)
    }

    @Test("parse: the same-note form [[#Heading]] is not a link")
    func parseSameNoteForm() throws {
        #expect(Wikilink(raw: "#Heading") == nil)
        #expect(Wikilink(raw: "#") == nil)
    }

    @Test("parse: an empty target with an alias is not a link; an empty alias is absent")
    func parseEmptyEdgeCases() throws {
        #expect(Wikilink(raw: "|alias") == nil)
        let link = Wikilink(raw: "a|")
        #expect(link?.target == "a")
        #expect(link?.alias == nil)
        #expect(link?.displayText == "a")
    }

    // MARK: - Resolution

    @Test("resolve: bare target matches basename case-insensitively, exactly one")
    func resolveBareCaseInsensitive() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("Note.md", "x", in: dir)

        guard case .resolved(let path) = bikeshed.resolve("note") else {
            Issue.record("expected resolved, got \(bikeshed.resolve("note"))")
            return
        }
        #expect(path == "Note.md")
        guard case .resolved(let upper) = bikeshed.resolve("NOTE") else {
            Issue.record("case variant should resolve")
            return
        }
        #expect(upper == "Note.md")
        // A trailing .md on the bare target is tolerated.
        guard case .resolved(let dotted) = bikeshed.resolve("note.md") else {
            Issue.record("bare target with .md should resolve")
            return
        }
        #expect(dotted == "Note.md")
    }

    @Test("resolve: zero matches is a broken link")
    func resolveZeroMatches() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a.md", "x", in: dir)

        #expect(bikeshed.resolve("missing") == .broken)
        #expect(bikeshed.resolve("a/x") == .broken)
    }

    @Test("resolve: a bare target in a subfolder resolves when unique")
    func resolveBareInSubfolder() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "x", in: dir)

        guard case .resolved(let path) = bikeshed.resolve("x") else {
            Issue.record("unique subfolder note should resolve")
            return
        }
        #expect(path == "a/x.md")
    }

    @Test("resolve: duplicate basenames are a broken link, never a silent match")
    func resolveDuplicateBasenames() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a/x.md", "1", in: dir)
        try stage("b/x.md", "2", in: dir)
        try stage("x.md", "3", in: dir)

        // Three notes named x: any bare [[x]] is broken.
        #expect(bikeshed.resolve("x") == .broken)
        #expect(bikeshed.resolve("X") == .broken)
        // Qualified still picks the one it names.
        guard case .resolved(let path) = bikeshed.resolve("b/x") else {
            Issue.record("qualified should resolve")
            return
        }
        #expect(path == "b/x.md")
    }

    @Test("resolve: qualified targets are vault-root-relative paths, case-insensitive")
    func resolveQualified() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("projects/plan.md", "p", in: dir)

        guard case .resolved(let path) = bikeshed.resolve("projects/plan") else {
            Issue.record("qualified should resolve")
            return
        }
        #expect(path == "projects/plan.md")
        guard case .resolved(let ci) = bikeshed.resolve("Projects/PLAN") else {
            Issue.record("case-insensitive qualified should resolve")
            return
        }
        #expect(ci == "projects/plan.md")
        guard case .resolved(let dotted) = bikeshed.resolve("projects/plan.md") else {
            Issue.record("qualified with .md should resolve")
            return
        }
        #expect(dotted == "projects/plan.md")
        #expect(bikeshed.resolve("other/plan") == .broken)
        #expect(bikeshed.resolve("projects/deep/plan") == .broken)
    }

    @Test("resolve: attachments are never linkable by wikilink")
    func resolveAttachmentsNever() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("photo.png", "img", in: dir)
        try stage("a/photo.png", "img2", in: dir)

        #expect(bikeshed.resolve("photo") == .broken)
        #expect(bikeshed.resolve("photo.png") == .broken)
        #expect(bikeshed.resolve("a/photo") == .broken)
    }

    // MARK: - Scanning (links(in:))

    @Test("scan: a wikilink outside code is found with range, form, and resolution")
    func scanBasic() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("Note.md", "x", in: dir)

        let text = "see [[Note]] here"
        let found = bikeshed.links(in: text)
        #expect(found.count == 1)
        guard let occ = found.first else { return }
        #expect(text[occ.range] == "[[Note]]")
        #expect(occ.link.target == "Note")
        #expect(occ.resolution == .resolved(path: "Note.md"))
        #expect(occ.line == text)
    }

    @Test("scan: broken links are reported as broken, not skipped")
    func scanBrokenLinks() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)

        let found = bikeshed.links(in: "[[Missing]]")
        #expect(found.count == 1)
        #expect(found.first?.resolution == .broken)
    }

    @Test("scan: fenced code blocks (``` and ~~~) are literal text")
    func scanFencedCode() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("Note.md", "x", in: dir)

        let backticks = "before\n```\n[[Note]]\n```\nafter [[Note]]"
        let tildes = "~~~swift\n[[Note]]\n~~~"
        #expect(bikeshed.links(in: backticks).count == 1, "only the real link")
        #expect(bikeshed.links(in: tildes).isEmpty)
    }

    @Test("scan: inline code is literal text")
    func scanInlineCode() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("Note.md", "x", in: dir)

        let text = "use `[[Note]]` not [[Note]]"
        let found = bikeshed.links(in: text)
        #expect(found.count == 1)
        #expect(text[found[0].range] == "[[Note]]")
        #expect(!text[found[0].range].hasPrefix("`"), "must not include the backtick")
    }

    @Test("scan: embeds ![[…]] are literal text")
    func scanEmbeds() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("Note.md", "x", in: dir)

        #expect(bikeshed.links(in: "![[Note]]").isEmpty)
        #expect(bikeshed.links(in: "x ![[Note]] and [[Note]]").count == 1)
    }

    @Test("scan: an unclosed [[ at end of line or file is literal text")
    func scanUnclosed() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("Note.md", "x", in: dir)

        // Unclosed on its line; the next line's link is still found.
        let twoLines = "a [[Note\nb [[Note]]"
        #expect(bikeshed.links(in: twoLines).count == 1)
        // Unclosed at end of file.
        #expect(bikeshed.links(in: "x [[Note").isEmpty)
        // A newline before the closing ]] is not a link.
        #expect(bikeshed.links(in: "[[A\nB]]").isEmpty)
    }

    @Test("scan: the first ]] closes; text after is literal")
    func scanFirstCloseWins() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("a.md", "x", in: dir)

        let found = bikeshed.links(in: "[[a]]b]] [[a]]")
        #expect(found.count == 2)
        #expect(found[0].link.target == "a")
    }

    @Test("scan: links inside blockquotes and list items are links")
    func scanInBlocks() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("Note.md", "x", in: dir)

        let text = "> quote [[Note]]\n- item [[Note]]"
        let found = bikeshed.links(in: text)
        #expect(found.count == 2)
        #expect(found[0].line == "> quote [[Note]]")
        #expect(found[1].line == "- item [[Note]]")
    }

    @Test("scan: indented code blocks are literal text")
    func scanIndentedCode() throws {
        let dir = tempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let bikeshed = Bikeshed(directory: dir)
        try stage("Note.md", "x", in: dir)

        let text = "    [[Note]]\nreal [[Note]]"
        #expect(bikeshed.links(in: text).count == 1)
    }
}
