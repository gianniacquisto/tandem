import Foundation

/// One project's memory health: how many curated notes it holds, how deep its
/// hidden saddlebag queue is, how old the oldest pending capture is, and when
/// the curated layer last moved. The readout the `note stats` verb reports —
/// derived live from the file set, never stored (spec: memory health).
public struct BikeshedProjectStats: Sendable, Equatable {
    /// The project folder name (one visible component under `projects/`).
    public let project: String
    /// Curated notes: `.md` files under `projects/<project>/`, hidden subtrees
    /// (the saddlebag and everything in it) pruned.
    public let curatedNotes: Int
    /// Pending captures: files under `projects/<project>/.raw/`, excluding the
    /// consumed `.done/` subtree.
    public let rawPending: Int
    /// Consumed captures: files under `projects/<project>/.raw/.done/`.
    public let rawArchived: Int
    /// The oldest pending capture's UTC timestamp, ISO-8601 — the queue's age —
    /// or `nil` when nothing is pending.
    public let oldestPending: String?
    /// The newest curated-note mtime, ISO-8601, or `nil` when the project has no
    /// curated notes.
    public let lastConsolidated: String?
}

/// The memory-health readout for the whole bikeshed: the root path, one row per
/// project, and the summed totals. `note stats`' success value.
public struct BikeshedStats: Sendable, Equatable {
    /// The bikeshed root, absolute.
    public let bikeshed: String
    /// One row per project folder, alphabetical.
    public let projects: [BikeshedProjectStats]
    /// The sums across `projects`.
    public let totals: BikeshedStatsTotals
}

/// The bikeshed-wide sums of `BikeshedProjectStats` (`oldestPending` and
/// `lastConsolidated` have no total — a timestamp does not sum).
public struct BikeshedStatsTotals: Sendable, Equatable {
    public let curatedNotes: Int
    public let rawPending: Int
    public let rawArchived: Int
}

extension Bikeshed {

    /// `stats(project:)`: per-project memory health — the curated-note count,
    /// the hidden saddlebag's pending/archived depth and oldest-pending age,
    /// and the last time the curated layer moved — summed into totals. Read-only
    /// and lock-free: it reads the tree (the one command that deliberately looks
    /// *inside* the hidden `.raw/` layer) and writes nothing (spec: memory
    /// health). `project` narrows the rows to one folder name; a missing
    /// bikeshed, a missing `projects/` folder, or a name that matches no folder
    /// is empty, never an error.
    public func stats(project: String? = nil) throws -> OperationResult<BikeshedStats> {
        let rows = projectFolders()
            .filter { project == nil || $0.name == project }
            .map { projectStats(named: $0.name, at: $0.url) }
        let totals = BikeshedStatsTotals(
            curatedNotes: rows.reduce(0) { $0 + $1.curatedNotes },
            rawPending: rows.reduce(0) { $0 + $1.rawPending },
            rawArchived: rows.reduce(0) { $0 + $1.rawArchived })
        return OperationResult(BikeshedStats(
            bikeshed: directory.standardizedFileURL.path,
            projects: rows,
            totals: totals))
    }

    // MARK: - Project discovery (private)

    /// Every project folder: a visible directory directly under `projects/`,
    /// alphabetical. A missing `projects/` folder is an empty list.
    private func projectFolders() -> [(name: String, url: URL)] {
        let fm = FileManager.default
        let root = directory.appendingPathComponent("projects")
        let entries = (try? fm.contentsOfDirectory(
            at: root, includingPropertiesForKeys: [.isDirectoryKey])) ?? []
        var projects: [(name: String, url: URL)] = []
        for url in entries {
            let name = url.lastPathComponent
            guard !name.hasPrefix(".") else { continue }
            guard
                let values = try? url.resourceValues(forKeys: [.isDirectoryKey]),
                values.isDirectory == true
            else { continue }
            projects.append((name, url))
        }
        projects.sort { ($0.name.lowercased(), $0.name) < ($1.name.lowercased(), $1.name) }
        return projects
    }

    /// One project's row, computed from its folder. Curated notes are counted
    /// recursively with hidden subtrees pruned (so `.raw/` and its `.done/` are
    /// out); the queue counts the files under `.raw/` and under `.raw/.done/`.
    private func projectStats(named name: String, at url: URL) -> BikeshedProjectStats {
        let curated = files(under: url, recursive: true, pruningHidden: true)
            .filter { $0.lastPathComponent.lowercased().hasSuffix(".md") }
        let rawRoot = url.appendingPathComponent(".raw")
        let pending = files(under: rawRoot, recursive: true, pruningHidden: true)
        let archived = files(
            under: rawRoot.appendingPathComponent(".done"), recursive: true, pruningHidden: false)

        let oldest = pending
            .compactMap { captureTimestamp(of: $0) ?? contentModifiedAt(of: $0) }
            .min()
        let last = curated.compactMap { contentModifiedAt(of: $0) }.max()

        return BikeshedProjectStats(
            project: name,
            curatedNotes: curated.count,
            rawPending: pending.count,
            rawArchived: archived.count,
            oldestPending: oldest.map(Timestamp.encode),
            lastConsolidated: last.map(Timestamp.encode))
    }

    // MARK: - Filesystem helpers (private)

    /// The regular files under `dir` (recursively when asked). A missing
    /// directory is an empty list. Hidden entries are skipped when
    /// `pruningHidden` (a hidden folder prunes its whole subtree, matching the
    /// bikeshed's own rule that hidden files are ignored).
    private func files(under dir: URL, recursive: Bool, pruningHidden: Bool) -> [URL] {
        let fm = FileManager.default
        guard
            let entries = try? fm.contentsOfDirectory(
                at: dir, includingPropertiesForKeys: [.isDirectoryKey])
        else {
            return []
        }
        var found: [URL] = []
        for url in entries {
            if pruningHidden, url.lastPathComponent.hasPrefix(".") { continue }
            guard
                let values = try? url.resourceValues(forKeys: [.isDirectoryKey]),
                let isDir = values.isDirectory
            else { continue }
            if isDir {
                if recursive {
                    found.append(contentsOf: files(
                        under: url, recursive: true, pruningHidden: pruningHidden))
                }
            } else {
                found.append(url)
            }
        }
        return found
    }

    /// A capture file's own UTC timestamp, decoded from its filename's leading
    /// `<utc>` stamp (`20261020T145459Z-…`), or `nil` when the name does not
    /// carry one (a hand-placed file). `Timestamp` renders ISO-8601; this is the
    /// filename's compact form.
    private func captureTimestamp(of url: URL) -> Date? {
        let name = url.lastPathComponent
        guard name.count >= 16 else { return nil }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "UTC")
        formatter.dateFormat = "yyyyMMdd'T'HHmmss'Z'"
        return formatter.date(from: String(name.prefix(16)))
    }

    /// A file's on-disk modification time, or `nil` when it cannot be read.
    private func contentModifiedAt(of url: URL) -> Date? {
        (try? url.resourceValues(forKeys: [.contentModificationDateKey]))?
            .contentModificationDate
    }
}
