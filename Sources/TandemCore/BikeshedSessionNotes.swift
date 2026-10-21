import Foundation

/// The notes a session wrote — derived from the files, never stored.
extension Bikeshed {

    /// `sessionNotes(session:)`: the capture paths a session wrote — every file
    /// under `projects/*/.raw/**/<session>/**`, the pending captures and the
    /// consumed ones alike.
    ///
    /// **Derived, not stored.** The session id is the folder a capture landed
    /// in (`projects/<project>/.raw/<session>/…`), and the consolidation pass
    /// moves what it consumes to `projects/<project>/.raw/.done/<session>/` —
    /// the same folder name in the archive. So *the notes a session wrote* is
    /// a scan of the file set, and the link cannot drift from the files: a
    /// capture that was consolidated is still named, and a note the human
    /// deleted is simply not there.
    ///
    /// Read-only and lock-free: it reads the tree (like `note stats`, the
    /// command that deliberately looks *inside* the hidden `.raw/` layer) and
    /// writes nothing. A missing bikeshed, a missing `projects/` folder, or a
    /// session that wrote nothing is an empty list, never an error.
    ///
    /// The paths are bikeshed-root-relative, exactly as `capture` returns them.
    ///
    /// **A capture filed under a different id is not this session's.** The
    /// capture verb falls back to a generated token when the host exports no
    /// session id (ADR 0016), and such a capture lands in a folder no record
    /// names. The scan cannot see across that gap — the note is unattributed
    /// by construction, and the face that draws it says so.
    public func sessionNotes(session: String) throws -> OperationResult<[String]> {
        OperationResult(capturePaths(for: session))
    }

    // MARK: - The scan (private)

    /// The capture files whose session folder is `session`, bikeshed-root
    /// relative, sorted.
    ///
    /// The session id is matched as a **folder name** — the name the capture
    /// was filed under — so a file that merely carries the id in its own name
    /// is not one of the session's notes.
    private func capturePaths(for session: String) -> [String] {
        var found: [String] = []
        for project in projectFolders() {
            let raw = project.url.appendingPathComponent(".raw")
            let root = raw.standardizedFileURL.path
            for url in files(under: raw, recursive: true, pruningHidden: false) {
                let path = url.standardizedFileURL.path
                guard path.hasPrefix(root + "/") else { continue }
                let relative = String(path.dropFirst(root.count + 1))
                let components = relative.split(separator: "/").map(String.init)
                // The session folder is a directory: a file's own name is
                // never the session.
                guard components.count > 1,
                      components.dropLast().contains(session)
                else { continue }
                found.append("projects/\(project.name)/.raw/\(relative)")
            }
        }
        return found.sorted()
    }
}
