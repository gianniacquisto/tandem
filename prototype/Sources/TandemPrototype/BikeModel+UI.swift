import Foundation

// PROTOTYPE — UI-side extensions on the in-memory bikeshed model. The model
// file (BikeshedModel.swift) carries the ticket semantics; these add only
// what the face needs on top (task toggling, note creation, backlink
// context lines).

extension BikeModel {

    /// Task-list checkbox click (ticket 05): `- [ ]` ↔ `- [x]`, one-character
    /// transaction; the buffer stays the source of truth.
    func toggleTask(notePath: String, line: Int) {
        guard let i = notes.firstIndex(where: { $0.path == notePath }) else { return }
        var lines = notes[i].body.components(separatedBy: "\n")
        guard line < lines.count else { return }
        if lines[line].contains("[ ]") {
            lines[line] = lines[line].replacingOccurrences(of: "[ ]", with: "[x]", options: .literal)
        } else if lines[line].contains("[x]") {
            lines[line] = lines[line].replacingOccurrences(of: "[x]", with: "[ ]", options: .literal)
        }
        notes[i].body = lines.joined(separator: "\n")
    }

    /// "New note" — creates `Untitled.md` (deduped) at the vault root and
    /// opens it. The prototype keeps it root-level; placement inside folders
    /// is a spec detail, not a surface question.
    @discardableResult
    func newNote() -> String {
        var name = "Untitled"
        var n = 2
        func taken(_ p: String) -> Bool { notes.contains { $0.path == p } }
        while taken("\(name).md") { name = "Untitled \(n)"; n += 1 }
        let path = "\(name).md"
        notes.append(BikeNote(path: path, body: "# \(name)\n"))
        openPath = path
        return path
    }

    /// The first line of `note` containing a wikilink that resolves to
    /// `targetPath` — the context row of a backlink entry.
    func backlinkContext(from note: BikeNote, to targetPath: String) -> String? {
        for raw in note.body.components(separatedBy: "\n") {
            for inner in Self.wikilinkTargets(in: raw) {
                let target = String(inner.split(whereSeparator: { $0 == "#" || $0 == "|" }).first ?? "")
                if resolve(target) == targetPath {
                    return raw.trimmingCharacters(in: .whitespaces)
                }
            }
        }
        return nil
    }
}
