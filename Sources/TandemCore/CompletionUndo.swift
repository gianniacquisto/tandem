import Foundation

/// The undo a completion leaves behind: **one** at a time, and it expires.
///
/// Completing a ticket is one click and immediate — and it erases the stage the
/// ticket came from, because `done` is a Status value and there is no previous
/// stage to restore (ADR 0017). A mis-click therefore costs a re-triage, not one
/// click back. The undo is the way back: a brief offer, not a history — the
/// newest completion replaces the last one, and the offer runs out after
/// `window` seconds.
///
/// The offer is raised by the *act* of completing, never by observing a
/// completion: a ticket completed elsewhere (the CLI, a hand edit) arrives as a
/// reload, and raises nothing.
public struct CompletionUndo: Sendable, Hashable {
    /// How long the offer stays open: six seconds. A completion is not a modal
    /// decision — the toast is there to catch a mis-click, not to be read.
    public static let window: TimeInterval = 6

    /// The ticket to put back.
    public let id: UUID
    /// Its title, so the toast can name the ticket it is talking about.
    public let title: String
    /// When the completion happened — the offer's clock starts here.
    public let at: Date

    public init(id: UUID, title: String, at: Date = .now) {
        self.id = id
        self.title = title
        self.at = at
    }

    /// When the offer runs out.
    public var expiresAt: Date { at.addingTimeInterval(Self.window) }

    /// Whether the offer is still open at `now`.
    public func isLive(at now: Date = .now) -> Bool { now < expiresAt }
}
