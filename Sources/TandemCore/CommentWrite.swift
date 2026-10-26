import Foundation

extension Store {

    /// The app's write path: append the human's comment to the ticket's log and
    /// hand back the record that was written.
    ///
    /// The author is the human: the app is the human's face, and the comment
    /// only the human can make is the one the app is here to record (ADR 0017's
    /// log). The CLI's `ticket comment add` names its own author — both faces
    /// write the same row in the same transaction.
    ///
    /// An empty (or whitespace-only) body is refused by the core —
    /// `DomainError.emptyCommentBody` — exactly as `todo comment add` refuses
    /// it, and **nothing is written**: the human's own text stays where it was
    /// typed, so it can be fixed and posted again. An unknown id fails the way
    /// `complete` fails, carrying the open list.
    ///
    /// **One transaction, one answer.** The append knows what it wrote — the
    /// author, the time and the body — so the record it wrote is what comes
    /// back; the log is not read again. The sheet's own read path
    /// (`Store.commentRead(ticketID:)`) carries its own `.busy` and `.failed`
    /// cases.
    ///
    /// The result's `note` is a recovery note, present only when this write
    /// recovered a corrupt store (ADR 0001).
    public func commentWrite(ticketID: String, body: String) throws -> OperationResult<Comment> {
        try append(ticketID: ticketID, author: TicketQuery.human, body: body)
    }
}
