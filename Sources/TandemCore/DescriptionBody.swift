import Foundation

/// The body of a `todo set --description -` write — the text a caller piped in.
///
/// The store's own rule is that an empty description is a legitimate value: the
/// text is trimmed at its edges and an empty result means "no description", so
/// `--description ""` clears the field deliberately (ADR 0017). The stdin path
/// is a different thing: an empty pipe, a failed heredoc and an unset variable
/// all arrive as the same empty body, and writing it would erase the brief the
/// session-ticket protocol exists to preserve (ADR 0024) — silently, with
/// exit 0 and no warning.
///
/// So a stdin body must carry text. An empty or whitespace-only body is
/// refused with `DomainError.emptyDescriptionBody`, the way `todo comment add`
/// refuses an empty comment body: a domain failure with actionable text, not a
/// silent write. The refusal belongs to the stdin path alone — the store still
/// takes an empty description as a deliberate clear.
public enum DescriptionBody {

    /// The body to write: the text as it arrived, trimmed at its edges by the
    /// store's own description rule. Throws `DomainError.emptyDescriptionBody`
    /// when the body carries no text.
    public static func required(_ body: String) throws -> String {
        guard let text = Store.normalizeDescription(body) else {
            throw DomainError.emptyDescriptionBody
        }
        return text
    }
}
