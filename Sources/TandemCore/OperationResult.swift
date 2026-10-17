import Foundation

/// The result of a core operation: a domain payload plus an optional recovery note.
///
/// The `note` is present only when this call triggered corrupt-store recovery
/// (ADR 0001: any operation — including reads — can be the one to recover). The GUI
/// surfaces it as a notification; the CLI carries it as a top-level `note` in
/// the result. Domain failures are *thrown* `DomainError`s, not carried here.
public struct OperationResult<T: Sendable>: Sendable {
    public let value: T
    public let note: String?

    public init(_ value: T, note: String? = nil) {
        self.value = value
        self.note = note
    }
}
