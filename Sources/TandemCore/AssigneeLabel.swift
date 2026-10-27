import Foundation

/// A ticket's assignee, as every face draws it: `nil` for unassigned, `human` for the
/// reserved word, `@name` for an agent. The name is the stored one, never normalised —
/// one rule shared by the board's tile, the sheet's picker and the list's row, so the
/// faces cannot label the same ticket differently.
public enum AssigneeLabel {
    public static func of(_ assignee: String?) -> String? {
        guard let assignee else { return nil }
        return assignee == TicketQuery.human ? TicketQuery.human : "@\(assignee)"
    }
}
