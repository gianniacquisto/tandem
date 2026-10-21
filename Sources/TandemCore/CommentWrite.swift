import Foundation

extension Store {

    /// The app's write path: append the human's comment to the ticket's log and
    /// hand back the log as it now reads.
    ///
    /// The author is the human: the app is the human's face, and the comment
    /// only the human can make is the one the app is here to record (ADR 0017's
    /// sidecar). The CLI's `todo comment add` names its own author and keeps its
    /// own path — this is the app's half of the same log, and the two write the
    /// same file.
    ///
    /// An empty (or whitespace-only) body is refused by the core —
    /// `DomainError.emptyCommentBody` — exactly as `todo comment add` refuses
    /// it, and **nothing is written**: the human's own text stays where it was
    /// typed, so it can be fixed and posted again. An unknown id fails the way
    /// `complete` fails, carrying the open list.
    ///
    /// The log is read back and handed over whole, so the app draws the log
    /// itself rather than a copy of it: one log, one truth, and what the app
    /// shows is what `todo comment list` shows.
    ///
    /// The result's `note` is a recovery note, present only when this write
    /// recovered a corrupt store (ADR 0001).
    public func commentWrite(ticketID: String, body: String) throws -> OperationResult<CommentRead> {
        let written = try append(ticketID: ticketID, author: TicketQuery.human, body: body)
        return OperationResult(commentRead(ticketID: ticketID), note: written.note)
    }
}
