import Foundation
import Observation

// PROTOTYPE — throwaway in-memory bikeshed model (bikeshed map, ticket 02).
//
// Mirrors the resolved ticket semantics so the face can be judged at decision
// fidelity — this is NOT the real `Bikeshed` type (that is the spec's job):
//   · note = .md file, filename = identity (ticket 04)
//   · attachments = non-.md files, invisible in the tree (ticket 04)
//   · wikilink resolution: bare → unique basename, qualified → unique
//     vault-root path, case-insensitive, exactly one (ticket 03)
//   · backlinks: notes containing a wikilink that resolves to the open note;
//     broken links contribute nothing (ticket 03)
//   · broken-link click → create the note at its path (ticket 03)

struct BikeNote: Identifiable, Equatable {
    /// Vault-root-relative path, forward slashes, includes `.md`.
    var path: String
    var body: String

    var id: String { path }

    /// Basename minus extension — the note's display name.
    var title: String {
        let base = path.split(separator: "/").last.map(String.init) ?? path
        return base.hasSuffix(".md") ? String(base.dropLast(3)) : base
    }

    /// Folder part, "" at the vault root.
    var folder: String {
        let parts = path.split(separator: "/")
        return parts.count > 1 ? String(parts.dropLast().last!) : ""
    }

    /// Vault-root-relative path minus the `.md` extension.
    var pathWithoutExt: String {
        path.hasSuffix(".md") ? String(path.dropLast(3)) : path
    }
}

struct BikeSearchResult: Identifiable {
    var note: BikeNote
    var snippet: String
    var id: String { note.path }
}

enum BikeSearchStyle: Int, CaseIterable {
    case tree, bar, panel
    var label: String { ["in tree", "in bar", "panel"][rawValue] }
}

enum BikeBacklinkStyle: Int, CaseIterable {
    case bottom, side
    var label: String { ["bottom", "side"][rawValue] }
}

/// One row of the file tree.
struct BikeTreeItem: Identifiable {
    enum Kind { case folder, note }
    var kind: Kind
    var name: String
    var path: String
    var id: String { path }
}

@Observable
final class BikeModel {
    var notes: [BikeNote] = []
    /// Non-.md files. Invisible in the tree; only reachable by relative path
    /// from the note that references them (ticket 04).
    var attachments: Set<String> = []

    var openPath: String?
    var query: String = ""
    var searchStyle: BikeSearchStyle = .tree
    var backlinkStyle: BikeBacklinkStyle = .bottom
    var expanded: Set<String> = []

    init() { seed() }

    // MARK: tree

    /// Children of `folder` ("", vault root): folders first, then notes,
    /// each alphabetical. Hidden files and attachments never appear (04).
    func treeChildren(of folder: String) -> [BikeTreeItem] {
        let prefix = folder.isEmpty ? "" : folder + "/"
        var folders: Set<String> = []
        for note in notes where note.path.hasPrefix(prefix) {
            let rest = String(note.path.dropFirst(prefix.count))
            if let slash = rest.firstIndex(of: "/") {
                folders.insert(String(rest[..<slash]))
            }
        }
        let folderItems = folders.sorted().map {
            BikeTreeItem(kind: .folder, name: $0, path: prefix + $0)
        }
        let noteItems = notes
            .filter { note in
                guard note.path.hasPrefix(prefix) else { return false }
                let rest = String(note.path.dropFirst(prefix.count))
                return !rest.contains("/")
            }
            .sorted { $0.title.lowercased() < $1.title.lowercased() }
            .map { BikeTreeItem(kind: .note, name: $0.title, path: $0.path) }
        return folderItems + noteItems
    }

    func toggleFolder(_ path: String) {
        if expanded.contains(path) { expanded.remove(path) } else { expanded.insert(path) }
    }

    // MARK: notes

    var openNote: BikeNote? { notes.first { $0.path == openPath } }

    func open(_ path: String) {
        openPath = path
        if let folder = notes.first(where: { $0.path == path })?.folder, !folder.isEmpty {
            expanded.insert(folder)
        }
    }

    /// Click on a wikilink target: resolved → open; broken → create an empty
    /// note at the link's path (bare → vault root, qualified → that path)
    /// and open it. No confirm (ticket 03).
    func followWikilink(_ target: String) {
        let clean = target
            .split(whereSeparator: { $0 == "#" || $0 == "|" }).first.map(String.init) ?? ""
        guard !clean.isEmpty else { return }
        if let resolved = resolve(clean) {
            open(resolved)
            return
        }
        let path = clean.hasSuffix(".md") ? clean : clean + ".md"
        if !notes.contains(where: { $0.path == path }) {
            let title = path.split(separator: "/").last.map(String.init) ?? path
            let name = title.hasSuffix(".md") ? String(title.dropLast(3)) : title
            notes.append(BikeNote(path: path, body: "# \(name)\n"))
        }
        if let slash = path.lastIndex(of: "/") {
            expanded.insert(String(path[..<slash]))
        }
        openPath = path
    }

    // MARK: wikilinks (ticket 03, v1 grammar)

    /// `[[target[#fragment][|alias]]]` — first `#`, then first `|`; first
    /// `]]` closes. Returns the raw inner strings.
    static func wikilinkTargets(in text: String) -> [String] {
        var out: [String] = []
        var s = Substring(text)
        while let open = s.range(of: "[[") {
            let after = s[open.upperBound...]
            guard let close = after.firstRange(of: "]]") else { break }
            out.append(String(after[..<close.lowerBound]))
            s = after[close.upperBound...]
        }
        return out
    }

    /// Bare target → case-insensitive basename match; qualified (contains
    /// `/`) → case-insensitive vault-root path match. Resolves iff exactly
    /// one note matches; zero or multiple → broken (ticket 03).
    func resolve(_ target: String) -> String? {
        let t = target.lowercased()
        let matches = notes.filter { note in
            t.contains("/")
                ? note.pathWithoutExt.lowercased() == t
                : note.title.lowercased() == t
        }
        return matches.count == 1 ? matches[0].path : nil
    }

    // MARK: backlinks (ticket 03 data model)

    func backlinks(for path: String) -> [BikeNote] {
        notes.filter { note in
            note.path != path &&
            Self.wikilinkTargets(in: note.body).contains { resolve($0) == path }
        }
        .sorted { $0.path < $1.path }
    }

    // MARK: search — PROVISIONAL semantics (ticket 02 settles these):
    // case-insensitive substring over title + body; one snippet per hit;
    // results sorted by vault path.

    var searchActive: Bool { !query.trimmingCharacters(in: .whitespaces).isEmpty }

    var results: [BikeSearchResult] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !q.isEmpty else { return [] }
        return notes
            .compactMap { note -> BikeSearchResult? in
                let inTitle = note.title.lowercased().contains(q)
                let body = note.body.lowercased()
                guard inTitle || body.contains(q) else { return nil }
                let snippet: String
                if inTitle {
                    let firstLine = note.body.split(separator: "\n").first {
                        !$0.trimmingCharacters(in: .whitespaces).isEmpty
                    }
                    snippet = firstLine.map { String($0).replacingOccurrences(of: "#", with: "") } ?? "—"
                } else if let range = note.body.range(of: q, options: .caseInsensitive) {
                    // Slice from the original-case body (the match range is
                    // located case-insensitively, but the snippet is shown as
                    // written, not lowercased).
                    let from = note.body.index(range.lowerBound, offsetBy: -24,
                                               limitedBy: note.body.startIndex) ?? note.body.startIndex
                    snippet = String(note.body[from...].prefix(52)).trimmingCharacters(in: .whitespaces)
                } else {
                    snippet = "—"
                }
                return BikeSearchResult(note: note, snippet: snippet)
            }
            .sorted { $0.note.path < $1.note.path }
    }

    func openSearchResult(_ r: BikeSearchResult) {
        open(r.note.path)
        query = ""
    }

    // MARK: images (ticket 05: note-relative; missing → broken placeholder)

    func imageExists(_ ref: String, in notePath: String) -> Bool {
        if ref.hasPrefix("http://") || ref.hasPrefix("https://") { return false } // never fetched
        let noteFolder: String
        if let slash = notePath.lastIndex(of: "/") {
            noteFolder = String(notePath[..<slash])
        } else {
            noteFolder = ""
        }
        return attachments.contains(noteFolder + ref)
    }

    // MARK: seed

    func reset() { seed() }

    private func seed() {
        let welcome = """
        # Welcome to the bikeshed

        A plain folder of `.md` files at `~/.tandem/bikeshed/`. The app
        watches it live; agents read and write the same files.

        - [x] Ship the wayfinder map
        - [ ] Work the UI prototype ticket
        - [ ] Write the spec

        Start with [[Bikeshed spec]] — or [[Tandem]].

        ## Links

        Resolved: [[Projects/Tandem]] · broken: [[Q4 goals]] — click it,
        it creates the note.

        > Blockquotes render tinted, like this one.

        ![diagram](assets/diagram.png)
        """

        let tandem = """
        # Tandem

        The todo app. Store at `~/.tandem/todos.json` (ADR 0001); the
        bikeshed is its notes sibling (ADR 0004).

        ## Architecture

        | Surface | Technology |
        |---|---|
        | Main window | SwiftUI |
        | Menu bar | NSStatusItem |
        | Agent API | MCP |

        ```swift
        func complete(_ id: UUID) {
            todos[idx].isDone = true
        }
        ```

        Inline math: the soft ceiling is $n \\approx 10^6$ characters.

        ![flow](assets/flow.png)
        """

        let spec = """
        # Bikeshed spec

        Draft. Decisions come from the map tickets.

        ## Editor
        - WKWebView + CodeMirror 6, inline live preview
        - Fallback: the sanctioned split view

        ## Render set (ticket 05)
        - GFM floor
        - Math via KaTeX: $E = mc^2$
        - Local images, note-relative
        - Mermaid: v2

        $$
        \\int_0^1 x^2\\,dx = \\tfrac{1}{3}
        $$
        """

        let gestures = """
        # Menu bar gestures

        - Swipe down from the item → quick add
        - Double-click → open the main window
        - Long-press → the popover

        ![sketch](https://tandem.example/sketch.png)
        """

        notes = [
            BikeNote(path: "Welcome.md", body: welcome),
            BikeNote(path: "Projects/Tandem.md", body: tandem),
            BikeNote(path: "Projects/Bikeshed spec.md", body: spec),
            BikeNote(path: "Ideas/Menu bar gestures.md", body: gestures),
        ]
        attachments = ["assets/diagram.png"]
        expanded = ["Projects"]
        openPath = "Projects/Tandem.md"
        query = ""
        searchStyle = .tree
        backlinkStyle = .bottom
    }
}
