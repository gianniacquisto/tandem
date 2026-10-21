import Foundation

/// Promoting a todo into a bikeshed note — the reverse of `Store.addFromNote`
/// (the note → todo direction).
///
/// One operation, so the app and any future agent-facing surface behave
/// identically: the note is created from the ticket's title and description,
/// the ticket's `link` points at the new note, and nothing else about the
/// ticket changes — its status, lane, and place in the list are untouched.
/// The board keeps the work item; the note is where the prose lives.
///
/// The note lands at `projects/<lane>/<slug>.md` — the lane as the project's
/// folder, the title as the note's name — and is **created, never
/// overwritten**: a name already taken gets a `-2`, `-3`, … suffix. A ticket
/// that already links to a note is **refused**: that note is the human's own
/// prose now, and promoting again would orphan it.
///
/// The two writes are not one transaction — the store and the bikeshed have
/// their own locks — so the order is fixed: the note is written first and the
/// link set second. A failed link write takes the note back off the disk, so a
/// promotion either lands whole or not at all.
public struct TodoToNote: Sendable {

    /// The store the ticket lives in.
    public let store: Store
    /// The bikeshed the note is written to.
    public let bikeshed: Bikeshed

    public init(store: Store, bikeshed: Bikeshed) {
        self.store = store
        self.bikeshed = bikeshed
    }

    /// `promote(_:)` — make a bikeshed note out of the ticket with `id` and
    /// link the two. Returns the new note's bikeshed-root-relative path and
    /// the ticket as it now stands.
    ///
    /// The note is made of the ticket **as the store holds it**: the title is
    /// the note's heading, the description its body, the lane its folder. An
    /// unknown id throws `DomainError.unknownID`; a ticket that already links
    /// to a note throws `PromotionError.alreadyLinked`.
    public func promote(_ id: String) throws -> OperationResult<PromotedNote> {
        let listed = try store.list(.all)
        guard let uuid = UUID(uuidString: id),
              let todo = listed.value.first(where: { $0.id == uuid })
        else {
            throw DomainError.unknownID(id: id, open: listed.value.filter { !$0.done })
        }
        // Refuse before anything is written: a ticket that already links to a
        // note keeps that link, and that note.
        if let existing = todo.link {
            throw PromotionError.alreadyLinked(path: existing)
        }

        // The note first, the link second: a note without a link is a note the
        // human can still find and use, while a link without a note is a link
        // to nothing.
        let path = try bikeshed.writeNew(
            path: Self.path(lane: todo.lane, title: todo.title),
            text: Self.text(title: todo.title, body: todo.description))
        do {
            let (linked, note) = try store.mutate { contents -> (value: Todo, changed: Bool) in
                guard let i = contents.todos.firstIndex(where: { $0.id == uuid }) else {
                    throw DomainError.unknownID(id: id, open: contents.todos.filter { !$0.done })
                }
                // The link may have been set since the read above (another
                // face, an agent): a note is only linked to a ticket that has
                // no note yet.
                if let existing = contents.todos[i].link {
                    throw PromotionError.alreadyLinked(path: existing)
                }
                contents.todos[i].link = path
                return (contents.todos[i], true)
            }
            return OperationResult(PromotedNote(path: path, todo: linked),
                                   note: note ?? listed.note)
        } catch {
            // The ticket did not take the link, so the note it was meant for
            // is not wanted either — take it back off the disk.
            _ = try? bikeshed.delete(path: path)
            throw error
        }
    }

    // MARK: - The note

    /// The note's path: `projects/<lane>/<slug>.md`. The lane is the
    /// project's folder (a ticket in the unnamed default area lands in
    /// `projects/uncategorized/`); the slug is the title.
    static func path(lane: String?, title: String) -> String {
        let folder = lane.flatMap { slug($0) } ?? "uncategorized"
        return "projects/\(folder)/\(slug(title) ?? "untitled").md"
    }

    /// The note's text: the title as the heading, the description as the body.
    /// A ticket with no description gets a note with the heading alone — the
    /// prose starts there. The ticket's own state is not written into the note
    /// (a note carries prose, not todo state).
    static func text(title: String, body: String?) -> String {
        let heading = "# " + title.replacingOccurrences(of: "\n", with: " ")
        guard let body, !body.isEmpty else { return heading + "\n" }
        return heading + "\n\n" + body + "\n"
    }

    /// A filename slug: lowercased, every run of characters that is not a
    /// letter or a digit collapsed to a single `-`, trimmed of `-`, and capped
    /// at 60 characters. `nil` when nothing is left — a name of only
    /// punctuation has no slug.
    static func slug(_ text: String) -> String? {
        let collapsed = text.lowercased()
            .replacingOccurrences(of: "[^a-z0-9]+", with: "-", options: .regularExpression)
        let trimmed = collapsed.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        guard !trimmed.isEmpty else { return nil }
        return String(trimmed.prefix(60))
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
    }
}

/// A ticket promoted to a note: the new note's path, and the ticket as it now
/// stands (linked to that note).
public struct PromotedNote: Sendable, Equatable {
    /// The new note's bikeshed-root-relative path — the ticket's `link`.
    public let path: String
    /// The ticket after the promotion: same title, description, status, and
    /// lane; only the `link` is new.
    public let todo: Todo

    public init(path: String, todo: Todo) {
        self.path = path
        self.todo = todo
    }
}

/// The promotion's own failures — the ones neither domain's vocabulary covers.
public enum PromotionError: Error, Sendable, Equatable {
    /// The ticket already links to a note. Promoting it again would point the
    /// ticket at a second note and orphan the first — prose that may have been
    /// written by hand.
    case alreadyLinked(path: String)

    public var message: String {
        switch self {
        case .alreadyLinked(let path):
            return "This ticket already links to a note (\"\(path)\"). "
                + "Open that note, or remove the link before promoting again."
        }
    }
}

extension PromotionError: TandemError {}

extension Bikeshed {

    /// `writeNew(path:text:)` — write `text` to a **new** note at `path` — or
    /// at the first free `-2`, `-3`, … variant of it — and return the path
    /// written.
    ///
    /// Never overwrites: an existing note is left exactly as it is and the new
    /// note is filed beside it under a suffixed name. The pick and the write
    /// happen under the bikeshed lock, so two writers cannot pick the same
    /// name.
    func writeNew(path: String, text: String) throws -> String {
        try validateNewName(path)
        return try locked {
            // A folder that is really a file blocks every candidate: the path
            // is uncreatable, and the note says so.
            if let taken = fileCollision(in: path, includingNote: false) {
                throw BikeshedError.alreadyExists(path: taken)
            }
            var candidate = path
            var suffix = 2
            while fileExists(candidate) {
                candidate = Self.suffixed(path, suffix)
                suffix += 1
            }
            guard let url = fileURL(for: candidate) else {
                throw BikeshedError.invalidName(name: candidate)
            }
            try atomicWrite(Data(text.utf8), to: url)
            return candidate
        }
    }

    /// Whether a file already sits at `path` — as the filesystem sees it, so
    /// a case variant counts as taken.
    private func fileExists(_ path: String) -> Bool {
        guard let url = fileURL(for: path) else { return false }
        return FileManager.default.fileExists(atPath: url.path)
    }

    /// The `-2`, `-3`, … variant of a note path: `projects/tandem/foo.md` →
    /// `projects/tandem/foo-2.md`.
    static func suffixed(_ path: String, _ n: Int) -> String {
        guard path.lowercased().hasSuffix(".md") else { return "\(path)-\(n)" }
        return String(path.dropLast(3)) + "-\(n).md"
    }
}
