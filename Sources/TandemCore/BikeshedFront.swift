import Foundation

/// The wiki front page (4E5E9A0B): the reading face's home — one row per
/// project under `projects/`, each row the project's Overview with its
/// scaffold page set, its "where this left off" state line, and its
/// freshness stamp. Derived live from the file set (like search and
/// backlinks — no stored index); a missing `projects/` folder is an empty
/// front page, never an error. Lock-free readers.
///
/// The matching rules follow the scaffold convention (CONVENTIONS.md):
/// the Overview is the project folder's note whose name matches the folder's
/// (`code-rank` ↔ `Code Rank.md` — case, spaces, and hyphens normalized
/// away), the scaffold pages are `<Overview stem> Decisions|Risks|Processes.md`
/// at the project root in the fixed order (missing pages omitted), and group
/// sub-folders inside a project are neither rows nor pages. A project folder
/// without an Overview is skipped — it has no state to show. The hidden
/// saddlebag (`.raw/`) is never read: the curated layer is the only input.

/// One scaffold page of a project's page set: its fixed role and its
/// bikeshed-root-relative path.
public struct ScaffoldPage: Sendable, Equatable {
    public enum Role: String, Sendable {
        case overview
        case decisions
        case risks
        case processes

        /// The chip label the face draws.
        public var label: String {
            switch self {
            case .overview: return "Overview"
            case .decisions: return "Decisions"
            case .risks: return "Risks"
            case .processes: return "Processes"
            }
        }
    }

    public let role: Role
    /// The note's bikeshed-root-relative path.
    public let path: String

    public init(role: Role, path: String) {
        self.role = role
        self.path = path
    }
}

/// One project row of the front page.
public struct ProjectFront: Sendable, Equatable {
    /// The project's display name: the Overview's title line (leading `#`s
    /// stripped), falling back to the folder name when the first line is empty.
    public let name: String
    /// The project folder, bikeshed-root-relative (`projects/tandem`).
    public let folder: String
    /// The scaffold page set — the Overview first, then Decisions, Risks,
    /// Processes; only the pages that exist.
    public let pages: [ScaffoldPage]
    /// The first non-empty line after the Overview's `## Where left off`
    /// heading — the "where this left off" summary. Nil when the heading is
    /// absent (the core never parses more than this one section).
    public let stateLine: String?
    /// The Overview's on-disk mtime — the freshness stamp. The consolidation
    /// pass rewrites the Overview exactly when the state changes, so the mtime
    /// is when the project's picture was last drawn.
    public let freshness: Date?
}

extension Bikeshed {

    /// `frontPage()`: one row per project folder under `projects/`, sorted by
    /// project name (case-insensitive). Lock-free.
    public func frontPage() throws -> OperationResult<[ProjectFront]> {
        let projectsURL = directory.appendingPathComponent("projects", isDirectory: true)
        let fm = FileManager.default
        guard let entries = try? fm.contentsOfDirectory(
            at: projectsURL, includingPropertiesForKeys: [.isDirectoryKey])
        else {
            return OperationResult([])   // no projects/ folder — an empty front page
        }
        var rows: [ProjectFront] = []
        for entry in entries {
            let values = try? entry.resourceValues(forKeys: [.isDirectoryKey])
            guard values?.isDirectory == true else { continue }
            let folder = "projects/" + entry.lastPathComponent
            guard let row = projectFront(folder: folder, url: entry) else { continue }
            rows.append(row)
        }
        rows.sort { ($0.name.lowercased(), $0.name) < ($1.name.lowercased(), $1.name) }
        return OperationResult(rows)
    }

    /// One project's row, or nil when the folder has no Overview.
    private func projectFront(folder: String, url: URL) -> ProjectFront? {
        let fm = FileManager.default
        guard let entries = try? fm.contentsOfDirectory(
            at: url, includingPropertiesForKeys: [.isDirectoryKey])
        else { return nil }

        // The project root's own .md notes only — group sub-folders are
        // neither the Overview nor scaffold pages.
        let noteNames: [String] = entries.compactMap { entry -> String? in
            guard let values = try? entry.resourceValues(forKeys: [.isDirectoryKey]),
                  values.isDirectory == false,
                  Self.isNoteName(entry.lastPathComponent)
            else { return nil }
            return entry.lastPathComponent
        }

        let folderKey = Self.matchKey(url.lastPathComponent)
        // The Overview: the note whose name matches the folder's. Alphabetical
        // first when the (improbable) normalization collapses two names.
        guard
            let overviewName = noteNames
                .filter({ Self.matchKey(Self.stem($0)) == folderKey })
                .sorted(by: { Self.sortKey($0) < Self.sortKey($1) })
                .first
        else { return nil }
        let overviewPath = folder + "/" + overviewName
        let stem = Self.stem(overviewName)

        // The scaffold pages: `<Overview stem> <Page>.md` at the project root,
        // in the fixed order, only the ones that exist.
        var pages: [ScaffoldPage] = [.init(role: .overview, path: overviewPath)]
        for role in [ScaffoldPage.Role.decisions, .risks, .processes] {
            let wanted = Self.matchKey(stem + " " + role.rawValue)
            if let name = noteNames.first(where: { Self.matchKey(Self.stem($0)) == wanted }) {
                pages.append(.init(role: role, path: folder + "/" + name))
            }
        }

        // The state line + freshness from the Overview's on-disk content.
        let content = try? readNote(overviewPath)
        return ProjectFront(
            name: content.flatMap { Self.title(of: $0.text) } ?? Self.stem(overviewName),
            folder: folder,
            pages: pages,
            stateLine: content.flatMap { Self.whereLeftOff(of: $0.text) },
            freshness: content?.modifiedAt)
    }

    /// The matching key: lowercase, spaces and hyphens removed — so the
    /// folder `code-rank` and the note `Code Rank` collapse to one key.
    private static func matchKey(_ name: String) -> String {
        name.lowercased().filter { $0 != " " && $0 != "-" }
    }

    /// The alphabetical order within the project's notes: case-insensitive,
    /// raw name as the deterministic tie-break (the bikeshed's sort rule).
    private static func sortKey(_ name: String) -> (String, String) {
        (name.lowercased(), name)
    }

    /// Whether a filename is a note (ends `.md`, case-insensitive).
    private static func isNoteName(_ name: String) -> Bool {
        name.lowercased().hasSuffix(".md")
    }

    /// A note's stem: the filename with the `.md` off.
    private static func stem(_ name: String) -> String {
        name.hasSuffix(".md") ? String(name.dropLast(3)) : name
    }

    /// The note's title: the first line, leading `#`s stripped, whitespace
    /// trimmed. Nil when the result is empty (an all-`#` or blank first line).
    private static func title(of text: String) -> String? {
        guard let first = text.components(separatedBy: "\n").first else { return nil }
        let trimmed = first.drop { $0 == "#" }.trimmingCharacters(in: .whitespaces)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// The first non-empty line after the `## Where left off` heading, or nil.
    private static func whereLeftOff(of text: String) -> String? {
        var past = false
        for raw in text.components(separatedBy: "\n") {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if !past {
                if line == "## Where left off" { past = true }
                continue
            }
            if line.isEmpty { continue }
            return line
        }
        return nil
    }
}
