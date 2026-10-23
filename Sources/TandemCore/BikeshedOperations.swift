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

    /// `list()`: the bikeshed tree — nested folders + `.md` notes, folders
    /// first, alphabetical within each level. Hidden files are excluded unless
    /// the human revealed them (`showsHiddenFiles`); non-`.md` attachments, and
    /// the lockfile (outside the bikeshed), are excluded always. A missing
    /// bikeshed folder is an empty tree, never an error. Lock-free.
    public func list() throws -> OperationResult<[BikeshedNode]> {
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
            try atomicWrite(Data(), to: url)
        }
        return OperationResult(path)
    }

    // MARK: - createFolder

    /// `createFolder(path)`: a new folder — no note is created. The name is
    /// validated before the lock with the same hygiene rules as a note name
    /// (`validateNewFolderName`): no `#` / `|` / malformed `/`, no hidden
    /// components — minus the `.md` requirement. Parent folders are created.
    /// A target that is already taken (a file or a folder) or an ancestor that
    /// is a file is `alreadyExists`, not an overwrite.
    public func createFolder(path: String) throws -> OperationResult<String> {
        try validateNewFolderName(path)   // validation throws before the lock
        guard let url = fileURL(for: path) else {
            throw BikeshedError.invalidName(name: path)   // unreachable: validated above
        }
        try locked {
            var isDir: ObjCBool = false
            if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) {
                throw BikeshedError.alreadyExists(path: path)
            }
            if let taken = fileCollision(in: path, includingNote: false) {
                throw BikeshedError.alreadyExists(path: taken)
            }
            do {
                try FileManager.default.createDirectory(
                    at: url, withIntermediateDirectories: true)
            } catch {
                throw BikeshedIOError.io("could not create folder \"\(path)\": \(error)")
            }
        }
        return OperationResult(path)
    }

    // MARK: - write

    /// `write(path, text)`: the autosave path — an atomic write under the
    /// bikeshed lock, storing exactly the bytes given (the core never parses
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
            try atomicWrite(data, to: url)
        }
        return OperationResult(WriteRecord(path: path, hash: Self.hash(of: data)))
    }

    // MARK: - rename

    /// `rename(from, to)`: change a note's path (rename = move) with link
    /// rewrite (ADR 0005). One transaction under the bikeshed lock: first
    /// rewrite every `[[wikilink]]` in the bikeshed that resolves to the old
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
            let postRename = Set(notes).subtracting([oldPath]).union([newPath])
            let mapping = [oldPath: Self.canonicalLinkTarget(for: newPath, in: postRename)]
            try applyLinkRewrite(to: notes, mapping: mapping)
            try fm.createDirectory(
                at: newURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            if Darwin.rename(oldURL.path, newURL.path) != 0 {
                throw BikeshedIOError.posix(op: "rename", code: errno)
            }
            return newPath
        }
        return OperationResult(newPathValue)
    }

    // MARK: - move

    /// `move(path, into:)`: move a note or a folder into another folder — the
    /// tree's drag-to-re-parent. The same machinery as `rename` (ADR 0005), in
    /// one locked transaction: every wikilink in the bikeshed that resolves to
    /// a moved note is rewritten to its new canonical target (bare when the
    /// new basename is unique, qualified when it is not) — bare, qualified,
    /// aliased and `#fragment` forms alike, in every note including the moved
    /// ones — and then a single `rename(2)` moves the note, or the folder
    /// with everything under it. A folder move rewrites the links to *every*
    /// note inside it.
    ///
    /// Refused, with nothing written:
    /// - a folder moved into itself or into one of its own descendants (a
    ///   cycle) — `BikeshedError.moveCycle`;
    /// - a name already taken in the destination folder (or a file where a
    ///   folder is needed) — `BikeshedError.alreadyExists`;
    /// - a missing source, or a destination that is not an existing folder —
    ///   `BikeshedError.unknownNote`.
    ///
    /// Moving an item into the folder it is already in is a no-op: the
    /// outcome is returned with no note moved and nothing written.
    public func move(path source: String, into destination: String) throws -> OperationResult<MoveOutcome> {
        guard let sourceURL = fileURL(for: source) else {
            throw BikeshedError.invalidName(name: source)
        }
        let destComponents = try Self.folderComponents(destination)
        let outcome = try locked {
            let fm = FileManager.default
            var sourceIsDir: ObjCBool = false
            guard fm.fileExists(atPath: sourceURL.path, isDirectory: &sourceIsDir) else {
                throw BikeshedError.unknownNote(path: source)
            }
            // The destination is an existing folder ("" is the bikeshed root).
            let destURL = destComponents.reduce(directory) { $0.appendingPathComponent($1) }
            var destIsDir: ObjCBool = false
            guard fm.fileExists(atPath: destURL.path, isDirectory: &destIsDir),
                  destIsDir.boolValue
            else {
                throw BikeshedError.unknownNote(path: destination)
            }
            // A folder cannot be moved into itself or into its own subtree.
            if sourceIsDir.boolValue, Self.isDescendant(destURL, of: sourceURL) {
                throw BikeshedError.moveCycle(path: source, into: destination)
            }
            // The paths, in the bikeshed's own spelling (on-disk case).
            let sourceRel = relativePath(of: sourceURL)
            let destRel = relativePath(of: destURL)
            let name = sourceURL.lastPathComponent
            let newPath = destRel.isEmpty ? name : destRel + "/" + name
            // Already there: nothing to move, nothing to write.
            if newPath == sourceRel {
                return MoveOutcome(path: newPath, oldPath: sourceRel,
                                   isFolder: sourceIsDir.boolValue, movedNotes: [])
            }
            guard let newURL = fileURL(for: newPath) else {
                throw BikeshedError.invalidName(name: newPath)
            }
            if fm.fileExists(atPath: newURL.path) {
                throw BikeshedError.alreadyExists(path: newPath)
            }
            if let taken = fileCollision(in: newPath, includingNote: false) {
                throw BikeshedError.alreadyExists(path: taken)
            }
            // The notes whose path changes: the note itself, or every note
            // under the moved folder.
            let notes = notePaths()
            let moves: [(old: String, new: String)]
            if sourceIsDir.boolValue {
                let prefix = sourceRel + "/"
                moves = notes.filter { $0.hasPrefix(prefix) }.map {
                    (old: $0, new: newPath + "/" + $0.dropFirst(prefix.count))
                }
            } else {
                moves = [(old: sourceRel, new: newPath)]
            }
            // Rewrite pass (per-file atomic), before the file moves:
            // resolution is against the pre-move file set, the canonical
            // targets against the post-move one.
            let postMove = Set(notes).subtracting(moves.map(\.old)).union(moves.map(\.new))
            var mapping: [String: String] = [:]
            for move in moves {
                mapping[move.old] = Self.canonicalLinkTarget(for: move.new, in: postMove)
            }
            try applyLinkRewrite(to: notes, mapping: mapping)
            try fm.createDirectory(
                at: newURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            if Darwin.rename(sourceURL.path, newURL.path) != 0 {
                throw BikeshedIOError.posix(op: "rename", code: errno)
            }
            return MoveOutcome(
                path: newPath,
                oldPath: sourceRel,
                isFolder: sourceIsDir.boolValue,
                movedNotes: moves.map { MovedNote(oldPath: $0.old, newPath: $0.new) })
        }
        return OperationResult(outcome)
    }

    // MARK: - delete

    /// `delete(path)`: removes the note — or folder, recursively — from the
    /// bikeshed under the lock. A permanent primitive that rewrites nothing:
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

    // MARK: - search

    /// `search(query)`: case-insensitive substring over title (the note's
    /// first line) + body (the rest — frontmatter included, the core never
    /// parses it). One row per note, not per match; a title hit wins and
    /// shows the first line; a body hit shows a snippet around the first
    /// match (±40 chars, whitespace collapsed, ellipses at sliced edges,
    /// original case). Sorted by bikeshed path. An empty or whitespace-only
    /// query yields no results; a missing bikeshed is an empty result.
    /// Lock-free.
    public func search(query: String) throws -> OperationResult<[SearchHit]> {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return OperationResult([]) }
        var hits: [SearchHit] = []
        for notePath in notePaths() {
            guard let content = try? readNote(notePath) else { continue }
            let lines = content.text.components(separatedBy: "\n")
            let title = lines.first ?? ""
            let body = lines.dropFirst().joined(separator: "\n")
            if title.range(of: trimmed, options: .caseInsensitive) != nil {
                hits.append(SearchHit(path: notePath, display: title))
            } else if let match = body.range(of: trimmed, options: .caseInsensitive) {
                hits.append(SearchHit(
                    path: notePath, display: Self.snippet(around: match, in: body)))
            }
        }
        hits.sort { ($0.path.lowercased(), $0.path) < ($1.path.lowercased(), $1.path) }
        return OperationResult(hits)
    }

    // MARK: - backlinks

    /// `backlinks(path)`: the bikeshed notes containing at least one wikilink
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
        let notes = notePaths()
        for notePath in notes {
            guard let content = try? readNote(notePath),
                  let first = Self.linkOccurrences(in: content.text, against: notes)
                      .first(where: { $0.resolution == .resolved(path: path) })
            else { continue }
            rows.append(Backlink(path: notePath, line: first.line))
        }
        rows.sort { ($0.path.lowercased(), $0.path) < ($1.path.lowercased(), $1.path) }
        return OperationResult(rows)
    }

    // MARK: - resolve (wikilink target)

    /// `resolve(target)`: where a wikilink target points, computed live from
    /// the current bikeshed file set — derived state, no stored link index
    /// (spec: wikilink model). A bare target (no `/`) matches a note
    /// basename with the `.md` off; a qualified target matches the
    /// bikeshed-root-relative path with the `.md` off. Case-insensitive, with or
    /// without a trailing `.md` on the target. Exactly one match →
    /// `.resolved`; zero or multiple → `.broken`. Attachments (non-`.md`) are
    /// never linkable. Lock-free.
    public func resolve(_ target: String) -> LinkResolution {
        NoteIndex(notes: notePaths()).resolve(target)
    }

    // MARK: - links(in:)

    /// `links(in: text)`: every wikilink occurrence in a note's text — the
    /// token's range (brackets included), its parsed form, where it resolves
    /// against the current bikeshed, and the line it sits on. Scanner rules
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

/// A note set pre-indexed for wikilink resolution: built once per scan,
/// then each target is a dictionary lookup instead of a full walk — a
/// bikeshed-wide scan resolves one link per note against every note, which is
/// quadratic without the index. Bare targets index on the lowercased
/// basename (`.md` off); qualified targets on the lowercased bikeshed path
/// (`.md` off).
private struct NoteIndex {
    private let bare: [String: [String]]
    private let qualified: [String: [String]]

    init(notes: [String]) {
        var bare: [String: [String]] = [:]
        var qualified: [String: [String]] = [:]
        for note in notes {
            let lower = note.lowercased()
            var base = (lower as NSString).lastPathComponent
            if base.hasSuffix(".md") { base.removeLast(3) }
            bare[base, default: []].append(note)
            var path = lower
            if path.hasSuffix(".md") { path.removeLast(3) }
            qualified[path, default: []].append(note)
        }
        self.bare = bare
        self.qualified = qualified
    }

    /// The shared resolution rule (backlinks, rename-rewrite, and the
    /// public `resolve` all use it): exactly one note whose name matches
    /// the target — bare targets (no `/`) against the basename, qualified
    /// targets against the bikeshed-root-relative path — case-insensitive,
    /// with or without a trailing `.md` on the target. Zero or multiple
    /// matches → a broken link.
    func resolve(_ target: String) -> LinkResolution {
        var lowered = target.lowercased()
        if lowered.hasSuffix(".md") { lowered.removeLast(3) }
        let matches = target.contains("/") ? qualified[lowered] : bare[lowered]
        guard let single = matches?.first, matches?.count == 1 else { return .broken }
        return .resolved(path: single)
    }
}

// The wikilink scanner lives here, and the health report
// (`BikeshedHealth.swift`) uses the same two internal helpers — the scan
// (`linkOccurrences`) and the line offsets — so the health report and the
// rename-rewrite can never drift on what counts as a link. Everything else in
// this extension is private to the file.
extension Bikeshed {

    /// The link-rewrite pass over the whole bikeshed: in every note, every
    /// wikilink that resolves to a moved note is rewritten to its new
    /// canonical target. Per-file atomic, and it runs *before* the move —
    /// resolution is against the pre-move file set.
    private func applyLinkRewrite(to notes: [String], mapping: [String: String]) throws {
        guard !mapping.isEmpty else { return }
        for notePath in notes {
            guard let content = try? readNote(notePath),
                  let targetURL = fileURL(for: notePath),
                  let rewritten = Self.rewriteLinks(
                    in: content.text,
                    against: notes,
                    mapping: mapping)
            else { continue }
            try atomicWrite(Data(rewritten.utf8), to: targetURL)
        }
    }

    /// The components of a destination folder path, checked for path hygiene
    /// (no empty, `.` or `..` component). The bikeshed root is the empty
    /// path.
    private static func folderComponents(_ path: String) throws -> [String] {
        guard !path.isEmpty else { return [] }
        let components = path.split(separator: "/", omittingEmptySubsequences: false)
            .map(String.init)
        for component in components
        where component.isEmpty || component == "." || component == ".." {
            throw BikeshedError.invalidName(name: path)
        }
        return components
    }

    /// True when `url` is `ancestor` itself or sits inside it — a folder
    /// moved into its own subtree would make the tree cyclic.
    private static func isDescendant(_ url: URL, of ancestor: URL) -> Bool {
        let child = url.standardizedFileURL.path
        let parent = ancestor.standardizedFileURL.path
        return child == parent || child.hasPrefix(parent + "/")
    }

    /// The wikilink scan against an explicit note set (instead of the live
    /// file set): the rename-rewrite pass resolves against the pre-rename
    /// set. Core of the public `links(in:)`.
    static func linkOccurrences(in text: String, against notes: [String]) -> [LinkOccurrence] {
        let tokens = Self.wikilinkTokens(in: text)
        guard !tokens.isEmpty else { return [] }   // no parse needed: no tokens
        let code = Self.codeRanges(in: text)
        let index = NoteIndex(notes: notes)
        var occurrences: [LinkOccurrence] = []
        for token in tokens {
            guard let link = Wikilink(raw: token.content) else { continue }
            if code.contains(where: { $0.overlaps(token.range) }) { continue }
            occurrences.append(LinkOccurrence(
                range: token.range,
                link: link,
                resolution: index.resolve(link.target),
                line: Self.line(containing: token.range.lowerBound, in: text)))
        }
        return occurrences
    }

    /// The flat ranges of every code span in the text — fenced blocks
    /// (``` / ~~~), indented blocks, and inline code — the wikilink scanner's
    /// exemption set. Ranges come from the CommonMark parse
    /// (`CodeBlock` / `InlineCode` `.range`), converted to flat string
    /// indices via per-line byte starts.
    private static func codeRanges(in text: String) -> [Range<String.Index>] {
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
    private static func flatRange(
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
    private static func wikilinkTokens(in text: String) -> [WikilinkToken] {
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

    /// The display text for a body search hit: the slice of `text` around
    /// the first match (±40 chars, clamped at the edges), whitespace
    /// collapsed to single spaces, ellipsised where the slice cut the text.
    private static func snippet(around match: Range<String.Index>, in text: String) -> String {
        let context = 40
        let start = text.index(
            match.lowerBound, offsetBy: -context, limitedBy: text.startIndex) ?? text.startIndex
        let end = text.index(
            match.upperBound, offsetBy: context, limitedBy: text.endIndex) ?? text.endIndex
        let collapsed = String(text[start..<end])
            .replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespaces)
        return (start > text.startIndex ? "…" : "") + collapsed
            + (end < text.endIndex ? "…" : "")
    }

    /// The line of `text` containing `index` (without its newline).
    private static func line(containing index: String.Index, in text: String) -> String {
        let start = text[text.startIndex..<index]
            .lastIndex(of: "\n")
            .map { text.index(after: $0) } ?? text.startIndex
        let end = text[text.index(after: index)...]
            .firstIndex(of: "\n") ?? text.endIndex
        return String(text[start..<end])
    }

    /// The canonical wikilink target for a moved note against the
    /// post-move file set: the bare basename (`.md` off, on-disk case)
    /// when that basename is unique there, the bikeshed-root-relative path
    /// (`.md` off) when it is not.
    private static func canonicalLinkTarget(for newPath: String, in postMoveNotes: Set<String>) -> String {
        var bare = (newPath as NSString).lastPathComponent
        if bare.lowercased().hasSuffix(".md") { bare.removeLast(3) }
        let bareLower = bare.lowercased()
        let unique = postMoveNotes.filter {
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
    /// every wikilink token that resolves to a moved note with its new
    /// target, leaving everything after the target — fragment, alias, their
    /// order — byte-identical. Tokens inside a leading terminated `---`
    /// frontmatter block are skipped (frontmatter stays opaque, ADR 0004);
    /// the scanner already exempts code, embeds, and unclosed tokens. `nil`
    /// when nothing resolves to a moved note.
    private static func rewriteLinks(
        in text: String,
        against notes: [String],
        mapping: [String: String]
    ) -> String? {
        let occurrences = Self.linkOccurrences(in: text, against: notes)
        guard !occurrences.isEmpty else { return nil }   // no parse needed: no tokens
        let frontmatter = frontmatterEnd(in: text)
        var out = text
        // Reverse order: replacing later tokens first keeps earlier indices valid.
        for occ in occurrences.reversed() {
            guard case .resolved(let path) = occ.resolution,
                  let newTarget = mapping[path]
            else { continue }
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
    private static func frontmatterEnd(in text: String) -> String.Index? {
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
