import Foundation

/// How a read of a ticket's comment log ended: the log itself, or why it could
/// not be read.
///
/// The three cases mean different things to whoever reads them, so they stay
/// apart. An empty log is nothing to say. A **busy** store is a read that waited
/// out the lock and gave up — nothing is wrong with the log, and the read can
/// simply be tried again. A **damaged** log is a file to repair. A timeout that
/// reads as corruption sends the human to repair a file that is perfectly fine.
public enum CommentRead: Sendable, Equatable {
    /// The log's comments, in append order (oldest first). Empty when the
    /// ticket has no log — the common case, and one that draws no section.
    case list([Comment])
    /// The store was locked by another process for longer than the read waited
    /// (`LockedDirectory.lockTimeout`, ~2 s). The log is fine; the read can be
    /// tried again. The message is the core's own.
    case busy(String)
    /// The log could not be read — a damaged line, most often. The message is
    /// the core's own, and it names the file to repair.
    case failed(String)
}

extension Store {

    /// Reads a ticket's comment log and says how the read ended.
    ///
    /// This is the call the app's ticket sheet makes. The read runs under the
    /// store's own flock, so a CLI append landing in the same window makes it
    /// time out — and a timeout is the store being busy, not the log being
    /// damaged. The two failures get their own words, because they ask the
    /// human for different things: one asks for a retry, the other for a repair.
    public func commentRead(ticketID: String) -> CommentRead {
        do {
            return .list(try list(ticketID: ticketID).value)
        } catch DomainError.locked {
            // The store is held by another process. The log was never read, and
            // nothing about it is wrong.
            return .busy(DomainError.locked.message)
        } catch {
            // Everything else is the log's own failure — a damaged line, most
            // often — and it keeps the core's words, so the app and the CLI say
            // the same thing about the same file.
            return .failed(tandemErrorText(error))
        }
    }
}
