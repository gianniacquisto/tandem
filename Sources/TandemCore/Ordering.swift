import Foundation

/// Pure ordering arithmetic for drag-and-drop (the view's half of an ordering change:
/// the CLI's `todo move --index` names the index itself, while a drop names a row).
/// knows *which row* the human dropped onto, and this turns that into the `index` that
/// `move` wants.
///
/// Keeping it here (rather than in the view) buys two things a view cannot give itself:
/// it is provable against the real `move` in tests, **and** the two subtle rules live in
/// one place, next to the operation they feed.
public enum TicketOrdering {

    /// The `index` to hand to `Store.move(_:toLane:at:)` so the ticket `id` lands
    /// immediately before (`after == false`) or after `anchorID`, **within the anchor's
    /// own lane**. `nil` when either ticket is missing, or when the row was dropped on
    /// itself.
    ///
    /// Two rules this encodes:
    /// - the anchor is resolved **by id**, never by its position in a visible list, so a
    ///   filter that hides rows cannot land the drop somewhere else;
    /// - one is subtracted when the moving ticket already sat before the drop point,
    ///   because `move` removes the ticket before it inserts.
    public static func dropIndex(
        in todos: [Ticket],
        moving id: UUID,
        relativeTo anchorID: UUID,
        after: Bool
    ) -> Int? {
        guard id != anchorID,
              let anchor = todos.first(where: { $0.id == anchorID })
        else { return nil }

        let members = todos.filter { $0.lane == anchor.lane }
        guard let anchorIndex = members.firstIndex(where: { $0.id == anchorID }) else { return nil }

        var index = anchorIndex + (after ? 1 : 0)
        if let movingIndex = members.firstIndex(where: { $0.id == id }), movingIndex < index {
            index -= 1
        }
        return index
    }
}
