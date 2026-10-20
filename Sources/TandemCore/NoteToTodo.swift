import Foundation

/// The reverse of the todo→note link (ADR 0010): a piece of a note becomes a
/// ticket, and the ticket points back at the note it came from.
///
/// One core operation sits behind both of the editor's triggers — the "Send to
/// todos" menu item and the ⌘⇧T shortcut — so the two cannot drift: the
/// editor hands over *text* (the selection, or the caret's checkbox line) and
/// the core decides what a ticket made of that text looks like.
///
/// - **Title** — the text's first non-blank line, trimmed, with a leading
///   task-list marker (`- [ ] `) removed: a checkbox line's *text* is the
///   ticket's title, not its syntax.
/// - **Description** — the whole text, trimmed (markdown, as written).
/// - **Link** — the note's bikeshed-root-relative path (`projects/App.md`), the
///   same shape `setLink` stores (ADR 0010).
///
/// An empty selection is rejected: there is no title to file. **The note is
/// never touched** — the operation writes the store and nothing else, so the
/// note stays exactly as the human left it.
extension Store {

    /// `addFromNote(selection, notePath:lane:status:assignee:origin:)` — file a
    /// selection from a note as a ticket, linked to that note.
    ///
    /// The title is trimmed and must not be empty; the description and the link
    /// follow the store's own field rules (a trimmed description, a
    /// bikeshed-root-relative path). The ticket lands **on top of its lane**
    /// (`nil` is the default area) and is born in `status` (the default is
    /// `backlog`) — the same placement `add` gives it. The filer is stamped
    /// here and never touched again (ADR 0020).
    ///
    /// An empty or whitespace-only selection throws `DomainError.emptySelection`;
    /// a note path that is not bikeshed-root-relative throws
    /// `DomainError.invalidNotePath`.
    public func addFromNote(
        _ rawSelection: String,
        notePath: String,
        lane: String? = nil,
        status: Status = .backlog,
        assignee: String? = nil,
        origin: String = "human"
    ) throws -> OperationResult<Todo> {
        let selection = rawSelection.trimmingCharacters(in: .whitespacesAndNewlines)
        let title = Self.ticketTitle(from: selection)
        guard !title.isEmpty else { throw DomainError.emptySelection }
        let path = notePath.trimmingCharacters(in: .whitespacesAndNewlines)
        guard Self.isNoteLinkPath(path) else { throw DomainError.invalidNotePath(notePath) }
        let who = Self.normalizeAssignee(assignee)
        let filer = Self.normalizeOrigin(origin)
        let description = Self.normalizeDescription(selection)
        let (todo, note) = try mutate { contents -> (value: Todo, changed: Bool) in
            try contents.validateLane(lane)
            var todo = Todo(title: title,
                            description: description,
                            assignee: who,
                            origin: filer,
                            lane: lane,
                            link: path)
            // Born in a stage: `setStatus` stamps `completedAt` if that stage is done.
            todo.setStatus(status)
            contents.todos.insert(todo, at: contents.blockStart(of: lane))
            return (todo, true)
        }
        return OperationResult(todo, note: note)
    }

    // MARK: - Field rules

    /// The ticket title a piece of note text makes: the **first non-blank
    /// line**, trimmed, with a leading task-list marker removed. Blank lines
    /// are skipped rather than taken, so a selection that starts with a
    /// newline still titles the ticket after its first real line.
    static func ticketTitle(from text: String) -> String {
        for line in text.split(separator: "\n", omittingEmptySubsequences: false) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty else { continue }
            return strippingTaskMarker(trimmed)
        }
        return ""
    }

    /// The task-list marker at the start of a line, removed: `- [ ] `, `* [x] `,
    /// `1. [ ] ` (any bullet, any indentation). A checkbox line's *text* is the
    /// ticket's title; the syntax is the note's business, not the ticket's.
    /// The line is returned unchanged when it carries no marker.
    static func strippingTaskMarker(_ line: String) -> String {
        var rest = Substring(line)
        // Indentation.
        while let c = rest.first, c == " " || c == "\t" { rest = rest.dropFirst() }
        guard let bullet = rest.first else { return line }
        if bullet == "-" || bullet == "*" || bullet == "+" {
            rest = rest.dropFirst()
        } else if bullet.isNumber {
            // An ordered mark: "1." or "1)".
            while let c = rest.first, c.isNumber { rest = rest.dropFirst() }
            guard let c = rest.first, c == "." || c == ")" else { return line }
            rest = rest.dropFirst()
        } else {
            return line
        }
        // The gap between the bullet and the marker.
        guard rest.first == " " || rest.first == "\t" else { return line }
        while let c = rest.first, c == " " || c == "\t" { rest = rest.dropFirst() }
        // The marker itself: "[ ]", "[x]", "[X]".
        guard rest.hasPrefix("[") else { return line }
        rest = rest.dropFirst()
        guard let state = rest.first, state == " " || state == "x" || state == "X" else { return line }
        rest = rest.dropFirst()
        guard rest.hasPrefix("]") else { return line }
        rest = rest.dropFirst()
        // What follows the marker is the line's text.
        return rest.trimmingCharacters(in: .whitespaces)
    }

    /// Whether `path` is a legal bikeshed-root-relative path — the shape of a
    /// note link (ADR 0010): not absolute, and no empty, `.` or `..` component
    /// (ADR 0008's rule, without a root to resolve it under).
    static func isNoteLinkPath(_ path: String) -> Bool {
        guard !path.isEmpty, !path.hasPrefix("/") else { return false }
        let components = path
            .split(separator: "/", omittingEmptySubsequences: false)
            .map(String.init)
        guard !components.isEmpty else { return false }
        for component in components
        where component.isEmpty || component == "." || component == ".." {
            return false
        }
        return true
    }
}
