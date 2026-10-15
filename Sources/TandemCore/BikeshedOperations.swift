import Foundation
import Darwin
import Markdown

/// Contract-level bikeshed note operations (spec: "Bikeshed core —
/// operations"). These are the only public surface the GUI uses; they define
/// the shared semantics so the app and any future agent-facing surface behave
/// identically. Every operation returns `OperationResult<T>` (the Store
/// shape); domain failures are thrown `BikeshedError`s, I/O failures
/// `BikeshedIOError`.
///
/// All app writes run under the exclusive sidecar lock with orphan cleanup
/// first (ADR 0004); readers take no lock.
extension Bikeshed {

    // MARK: - list

    /// `list()`: the vault tree — nested folders + `.md` notes, folders
    /// first, alphabetical within each level. Hidden files, non-`.md`
    /// attachments, and the lockfile (outside the vault) are excluded. A
    /// missing vault folder is an empty tree, never an error. Lock-free.
    public func list() throws -> OperationResult<[VaultNode]> {
        return OperationResult(tree())
    }

    // MARK: - read

    /// `read(path)`: the note's text plus its on-disk hash (SHA-256 of the
    /// bytes; drives conflict detection and self-write dedup) and mtime
    /// (internal truth, not displayed). Lock-free. A missing path throws
    /// `BikeshedError.unknownNote`.
    public func read(path: String) throws -> OperationResult<NoteContent> {
        return OperationResult(try readNote(path))
    }

    // MARK: - create

    /// `create(path)`: a new empty note. The name is validated before the
    /// lock (no `#` / `|` / malformed `/`, no hidden components; note names
    /// end in `.md`; spaces allowed); parent folders are created. A target
    /// that is already taken — the note itself, or an ancestor that would
    /// have to be a folder — is an error, not an overwrite.
    public func create(path: String) throws -> OperationResult<String> {
        try validateNewName(path)   // validation throws before the lock
        guard let url = fileURL(for: path) else {
            throw BikeshedError.invalidName(name: path)   // unreachable: validated above
        }
        try locked {
            if let taken = fileCollision(in: path, includingNote: true) {
                throw BikeshedError.alreadyExists(path: taken)
            }
            try Self.atomicWrite(Data(), to: url)
        }
        return OperationResult(path)
    }

    // MARK: - write

    /// `write(path, text)`: the autosave path — an atomic write under the
    /// vault lock, storing exactly the bytes given (the core never parses
    /// note text; frontmatter passes through untouched). Creates parent
    /// folders and, for a not-yet-existing path, applies the full naming
    /// validation; an existing path (including pre-existing name violators)
    /// is edited in place. Returns what was written — path + hash — so the
    /// watcher can dedup self-writes. A failed write is an error; keeping the
    /// buffer is the model's problem, not the core's.
    public func write(path: String, text: String) throws -> OperationResult<WriteRecord> {
        guard let url = fileURL(for: path) else {
            throw BikeshedError.invalidName(name: path)
        }
        var isDir: ObjCBool = false
        let exists = FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir)
        if !exists {
            try validateNewName(path)   // a new note gets the full naming rules
        } else if isDir.boolValue {
            throw BikeshedIOError.io("path \"\(path)\" is a folder")
        }
        let data = Data(text.utf8)
        try locked {
            // An ancestor that is a file would need to be a folder: the path
            // is uncreatable, so refuse it (the note itself is the overwrite
            // target, not a collision).
            if let taken = fileCollision(in: path, includingNote: false) {
                throw BikeshedError.alreadyExists(path: taken)
            }
            try Self.atomicWrite(data, to: url)
        }
        return OperationResult(WriteRecord(path: path, hash: Self.hash(of: data)))
    }

    // MARK: - rename

    /// `rename(from, to)`: change a note's path (rename = move) with link
    /// rewrite (ADR 0005). One transaction under the vault lock: first
    /// rewrite every `[[wikilink]]` in the vault that resolves to the old
    /// path — every form (bare, qualified, aliased, fragment, any split
    /// order), in every note including the note itself — re-canonicalised
    /// against the post-rename file set (bare when the new basename is
    /// unique, qualified when it is not); wikilinks only — markdown links,
    /// embeds, code, and wikilinks inside a leading terminated `---`
    /// frontmatter block are byte-identical. Then a single `rename(2)` of
    /// the note file (same volume, parent folders created). No journal: a
    /// mid-sequence crash yields no torn files — at most broken links — and
    /// re-running the rename self-heals (the rewritten references already
    /// resolve to the new path). A missing source is `unknownNote`; a taken
    /// new path (or a file where a folder is needed) is `alreadyExists`
    /// with nothing written; the new name gets the full naming validation.
    public func rename(from oldPath: String, to newPath: String) throws -> OperationResult<String> {
        try validateNewName(newPath)   // validation throws before the lock
        guard let oldURL = fileURL(for: oldPath),
              let newURL = fileURL(for: newPath)
        else {
            throw BikeshedError.invalidName(name: oldPath)
        }
        let newPathValue = try locked {
            let fm = FileManager.default
            var oldIsDir: ObjCBool = false
            guard fm.fileExists(atPath: oldURL.path, isDirectory: &oldIsDir),
                  !oldIsDir.boolValue
            else {
                throw BikeshedError.unknownNote(path: oldPath)
            }
            var newIsDir: ObjCBool = false
            if fm.fileExists(atPath: newURL.path, isDirectory: &newIsDir) {
                throw BikeshedError.alreadyExists(path: newPath)
            }
            if let taken = fileCollision(in: newPath, includingNote: false) {
                throw BikeshedError.alreadyExists(path: taken)
            }
            // Rewrite pass (per-file atomic), before the file moves:
            // resolution is against the pre-rename file set.
            let notes = notePaths()
            let newTarget = Self.canonicalLinkTarget(for: newPath, in: notes, replacing: oldPath)
            for notePath in notes {
                guard let content = try? readNote(notePath),
                      let targetURL = fileURL(for: notePath),
                      let rewritten = Self.rewriteLinks(
                        in: content.text,
                        against: notes,
                        resolvingTo: oldPath,
                        as: newTarget)
                else { continue }
                try Self.atomicWrite(Data(rewritten.utf8), to: targetURL)
            }
            try fm.createDirectory(
                at: newURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            if Darwin.rename(oldURL.path, newURL.path) != 0 {
                throw BikeshedIOError.posix(op: "rename", code: errno)
            }
            return newPath
        }
        return OperationResult(newPathValue)
    }

    // MARK: - delete

    /// `delete(path)`: removes the note — or folder, recursively — from the
    /// vault under the lock. A permanent primitive that rewrites nothing:
    /// links to a deleted note become broken (no tombstones, ADR 0004). The
    /// GUI performs the macOS Trash move (recoverable) ahead of this; this is
    /// the fallback the Trash move falls back to. A missing path throws
    /// `BikeshedError.unknownNote`.
    public func delete(path: String) throws -> OperationResult<Void> {
        guard let url = fileURL(for: path) else {
            throw BikeshedError.invalidName(name: path)
        }
        try locked {
            var isDir: ObjCBool = false
            guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) else {
                throw BikeshedError.unknownNote(path: path)
            }
            do {
                try FileManager.default.removeItem(at: url)
            } catch {
                throw BikeshedIOError.io("could not delete \"\(path)\": \(error)")
            }
        }
        return OperationResult(())
    }

    // MARK: - backlinks

    /// `backlinks(path)`: the vault notes containing at least one wikilink
    /// that resolves to the given note — one row per linking note, each with
    /// the line of its first resolving wikilink (self-links included; broken
    /// links, code, and embeds contribute nothing). Derived state, recomputed
    /// from the current file set — no stored link index; the rows move with
    /// creation, deletion, and rename on their own. A missing (or folder)
    /// target throws `BikeshedError.unknownNote`. Lock-free.
    public func backlinks(path: String) throws -> OperationResult<[Backlink]> {
        guard let url = fileURL(for: path) else {
            throw BikeshedError.invalidName(name: path)
        }
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir),
              !isDir.boolValue
        else {
            throw BikeshedError.unknownNote(path: path)
        }
        var rows: [Backlink] = []
        for notePath in notePaths() {
            guard let content = try? readNote(notePath),
                  let first = links(in: content.text)
                      .first(where: { $0.resolution == .resolved(path: path) })
            else { continue }
            rows.append(Backlink(path: notePath, line: first.line))
        }
        rows.sort { ($0.path.lowercased(), $0.path) < ($1.path.lowercased(), $1.path) }
        return OperationResult(rows)
    }

    // MARK: - resolve (wikilink target)

    /// `resolve(target)`: where a wikilink target points, computed live from
    /// the current vault file set — derived state, no stored link index
    /// (spec: wikilink model). A bare target (no `/`) matches a note
    /// basename with the `.md` off; a qualified target matches the
    /// vault-root-relative path with the `.md` off. Case-insensitive, with or
    /// without a trailing `.md` on the target. Exactly one match →
    /// `.resolved`; zero or multiple → `.broken`. Attachments (non-`.md`) are
    /// never linkable. Lock-free.
    public func resolve(_ target: String) -> LinkResolution {
        Self.resolveLink(target, in: notePaths())
    }

    // MARK: - links(in:)

    /// `links(in: text)`: every wikilink occurrence in a note's text — the
    /// token's range (brackets included), its parsed form, where it resolves
    /// against the current vault, and the line it sits on. Scanner rules
    /// (spec: wikilink model): the first `]]` on the same line closes a `[[`
    /// (a newline before the close makes the `[[` literal text); a `[[`
    /// immediately preceded by `!` (an embed) is literal text; and anything
    /// inside a fenced, indented, or inline code span is literal text
    /// (detected with the CommonMark parser, so the exemption follows
    /// markdown's own code rules). Broken links are reported, not skipped —
    /// the GUI renders them distinctly.
    public func links(in text: String) -> [LinkOccurrence] {
        Self.linkOccurrences(in: text, against: notePaths())
    }
}

// MARK: - Wikilink resolution + scanning (private)

/// One `[[…]]` token found by the scanner: its range in the text (brackets
/// included) and the parsed content between the brackets.
private struct WikilinkToken {
    let range: Range<String.Index>
    let content: String
}

private extension Bikeshed {

    /// The wikilink scan against an explicit note set (instead of the live
    /// file set): the rename-rewrite pass resolves against the pre-rename
    /// set. Core of the public `links(in:)`.
    static func linkOccurrences(in text: String, against notes: [String]) -> [LinkOccurrence] {
        let code = Self.codeRanges(in: text)
        var occurrences: [LinkOccurrence] = []
        for token in Self.wikilinkTokens(in: text) {
            guard let link = Wikilink(raw: token.content) else { continue }
            if code.contains(where: { $0.overlaps(token.range) }) { continue }
            occurrences.append(LinkOccurrence(
                range: token.range,
                link: link,
                resolution: Self.resolveLink(link.target, in: notes),
                line: Self.line(containing: token.range.lowerBound, in: text)))
        }
        return occurrences
    }

    /// The shared resolution rule (backlinks, rename-rewrite, and the public
    /// `resolve` all use it): exactly one note whose name matches the target
    /// — bare targets against the basename (`.md` off), qualified targets
    /// (containing `/`) against the vault-root-relative path (`.md` off) —
    /// case-insensitive, with or without a trailing `.md` on the target.
    static func resolveLink(_ target: String, in notes: [String]) -> LinkResolution {
        var loweredTarget = target.lowercased()
        if loweredTarget.hasSuffix(".md") { loweredTarget.removeLast(3) }
        let qualified = target.contains("/")
        var matches: [String] = []
        for note in notes {
            var candidate = note.lowercased()
            if qualified {
                if candidate.hasSuffix(".md") { candidate.removeLast(3) }
            } else {
                candidate = (candidate as NSString).lastPathComponent
                if candidate.hasSuffix(".md") { candidate.removeLast(3) }
            }
            if candidate == loweredTarget {
                matches.append(note)
                if matches.count > 1 { return .broken }
            }
        }
        guard let single = matches.first else { return .broken }
        return .resolved(path: single)
    }

    /// The flat ranges of every code span in the text — fenced blocks
    /// (``` / ~~~), indented blocks, and inline code — the wikilink scanner's
    /// exemption set. Ranges come from the CommonMark parse
    /// (`CodeBlock` / `InlineCode` `.range`), converted to flat string
    /// indices via per-line byte starts.
    static func codeRanges(in text: String) -> [Range<String.Index>] {
        let lineStartBytes = lineStartByteOffsets(of: text)
        var ranges: [Range<String.Index>] = []
        func walk(_ node: Markup) {
            if (node is CodeBlock || node is InlineCode),
               let source = node.range,
               let flat = flatRange(source, lineStartBytes: lineStartBytes, text: text)
            {
                ranges.append(flat)
            }
            for child in node.children { walk(child) }
        }
        walk(Document(parsing: text))
        return ranges
    }

    /// The byte offset of the start of each line of `text` (line 1 at 0).
    static func lineStartByteOffsets(of text: String) -> [Int] {
        var starts: [Int] = [0]
        for (offset, byte) in text.utf8.enumerated() where byte == 0x0A {
            starts.append(offset + 1)
        }
        return starts
    }

    /// Convert a source range (1-based line + 1-based UTF-8 byte column) to a
    /// flat `String.Index` range. `nil` if the location is out of range.
    static func flatRange(
        _ source: SourceRange,
        lineStartBytes: [Int],
        text: String
    ) -> Range<String.Index>? {
        func index(of location: SourceLocation) -> String.Index? {
            let lineIndex = location.line - 1
            guard lineIndex >= 0, lineIndex < lineStartBytes.count else { return nil }
            let byteOffset = lineStartBytes[lineIndex] + location.column - 1
            guard byteOffset >= 0, byteOffset <= text.utf8.count else { return nil }
            let byteIndex = text.utf8.index(text.utf8.startIndex, offsetBy: byteOffset)
            return String.Index(byteIndex, within: text)
        }
        guard let lower = index(of: source.lowerBound),
              let upper = index(of: source.upperBound)
        else { return nil }
        return lower..<upper
    }

    /// Scan `text` for `[[…]]` tokens. The first `]]` on the same line
    /// closes a `[[`; a newline (or end of text) before the close leaves the
    /// `[[` as literal text; a `[[` immediately preceded by `!` (embed) is
    /// literal text. Returns tokens in document order.
    static func wikilinkTokens(in text: String) -> [WikilinkToken] {
        var tokens: [WikilinkToken] = []
        var i = text.startIndex
        while i < text.endIndex {
            guard text[i] == "[" else {
                i = text.index(after: i)
                continue
            }
            let secondIdx = text.index(after: i)
            guard secondIdx < text.endIndex, text[secondIdx] == "[" else {
                i = secondIdx
                continue
            }
            let openStart = i
            let afterOpen = text.index(after: secondIdx)
            if i > text.startIndex, text[text.index(before: i)] == "!" {
                i = afterOpen   // an embed: literal text
                continue
            }
            var j = afterOpen
            var closeIdx: String.Index?
            while j < text.endIndex {
                let c = text[j]
                if c == "\n" { break }
                if c == "]" {
                    let after = text.index(after: j)
                    if after < text.endIndex, text[after] == "]" {
                        closeIdx = j
                        break
                    }
                }
                j = text.index(after: j)
            }
            guard let close = closeIdx else {
                i = afterOpen   // unclosed on its line: literal text
                continue
            }
            let end = text.index(after: text.index(after: close))
            tokens.append(WikilinkToken(
                range: openStart..<end,
                content: String(text[afterOpen..<close])))
            i = end
        }
        return tokens
    }

    /// The line of `text` containing `index` (without its newline).
    static func line(containing index: String.Index, in text: String) -> String {
        let start = text[text.startIndex..<index]
            .lastIndex(of: "\n")
            .map { text.index(after: $0) } ?? text.startIndex
        let end = text[text.index(after: index)...]
            .firstIndex(of: "\n") ?? text.endIndex
        return String(text[start..<end])
    }

    /// The canonical wikilink target for a renamed note against the
    /// post-rename file set: the bare basename (`.md` off, on-disk case)
    /// when that basename is unique there, the vault-root-relative path
    /// (`.md` off) when it is not.
    static func canonicalLinkTarget(
        for newPath: String,
        in currentNotes: [String],
        replacing oldPath: String
    ) -> String {
        var bare = (newPath as NSString).lastPathComponent
        if bare.lowercased().hasSuffix(".md") { bare.removeLast(3) }
        let bareLower = bare.lowercased()
        let postRename = Set(currentNotes).subtracting([oldPath]).union([newPath])
        let unique = postRename.filter {
            var base = ($0 as NSString).lastPathComponent.lowercased()
            if base.hasSuffix(".md") { base.removeLast(3) }
            return base == bareLower
        }.count == 1
        if unique { return bare }
        var qualified = newPath
        if qualified.lowercased().hasSuffix(".md") { qualified.removeLast(3) }
        return qualified
    }

    /// The rename-rewrite for one note's text: replace the target part of
    /// every wikilink token that resolves to `resolvingTo` (against the
    /// pre-rename set `notes`) with `as`, leaving everything after the
    /// target — fragment, alias, their order — byte-identical. Tokens inside
    /// a leading terminated `---` frontmatter block are skipped (frontmatter
    /// stays opaque, ADR 0004); the scanner already exempts code, embeds,
    /// and unclosed tokens. `nil` when nothing resolves to the target.
    static func rewriteLinks(
        in text: String,
        against notes: [String],
        resolvingTo targetPath: String,
        as newTarget: String
    ) -> String? {
        let occurrences = Self.linkOccurrences(in: text, against: notes).filter {
            $0.resolution == .resolved(path: targetPath)
        }
        guard !occurrences.isEmpty else { return nil }
        let frontmatter = frontmatterEnd(in: text)
        var out = text
        // Reverse order: replacing later tokens first keeps earlier indices valid.
        for occ in occurrences.reversed() {
            if let frontmatter, occ.range.upperBound <= frontmatter { continue }
            let contentStart = text.index(occ.range.lowerBound, offsetBy: 2)   // past "[["
            let closeStart = text.index(occ.range.upperBound, offsetBy: -2)    // first "]" of "]]"
            let content = text[contentStart..<closeStart]
            let targetEnd = min(
                content.firstIndex(of: "#") ?? closeStart,
                content.firstIndex(of: "|") ?? closeStart)
            out.replaceSubrange(contentStart..<targetEnd, with: newTarget)
        }
        return out == text ? nil : out   // every hit was in frontmatter: no write
    }

    /// The index just past a leading, properly terminated `---` frontmatter
    /// block, or `nil` (no frontmatter, or unterminated — a bare `---`
    /// thematic break is not frontmatter).
    static func frontmatterEnd(in text: String) -> String.Index? {
        guard let firstNewline = text.firstIndex(of: "\n"),
              isFrontmatterDelimiter(text[text.startIndex..<firstNewline])
        else { return nil }
        var lineStart = text.index(after: firstNewline)
        while lineStart < text.endIndex {
            guard let newline = text[lineStart...].firstIndex(of: "\n") else {
                // A final line with no trailing newline can close the block.
                return isFrontmatterDelimiter(text[lineStart...]) ? text.endIndex : nil
            }
            if isFrontmatterDelimiter(text[lineStart..<newline]) {
                return text.index(after: newline)
            }
            lineStart = text.index(after: newline)
        }
        return nil
    }

    private static func isFrontmatterDelimiter(_ line: Substring) -> Bool {
        String(line.hasSuffix("\r") ? line.dropLast() : line) == "---"
    }
}
