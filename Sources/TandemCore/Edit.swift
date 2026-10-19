import Foundation

/// One field's change in a multi-field operation: leave it alone, or set it.
///
/// A plain `T?` cannot carry both meanings — `nil` would mean "clear this" and "do not
/// touch this" at once — and an operation that sets several fields in one write needs to
/// tell them apart. `.set(nil)` on an optional field clears it; `.unchanged` (the default
/// everywhere) is the absent instruction.
public enum Edit<T: Sendable>: Sendable {
    case unchanged
    case set(T)

    /// The value to set, or `nil` when the field is unchanged. For an optional field this
    /// is `T??`: `.some(nil)` is "clear it", `.none` is "leave it".
    public var newValue: T? {
        if case .set(let value) = self { return value }
        return nil
    }

    /// Whether the caller named this field at all — the question `newValue == nil` cannot
    /// answer for an optional field.
    public var isSet: Bool {
        if case .set = self { return true }
        return false
    }
}
