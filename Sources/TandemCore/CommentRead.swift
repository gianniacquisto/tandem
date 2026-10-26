import Foundation

/// How a read of a ticket's comment log ended: the log itself, or why it could
/// not be read.
///
/// The three cases mean different things to whoever reads them, so they stay
/// apart. An empty log is nothing to say. A **busy** store is a read that
/// waited out the store's ~2 s contention window and gave up — another
/// connection held the database exclusively (an exclusive-locking writer, or a
/// checkpoint), nothing is wrong with the log, and the read can simply be tried
/// again. A **damaged** legacy log is a file to repair. The database's normal
/// reads are lock-free WAL snapshots and never wait for an ordinary writer; the
/// case stays because the exclusive holder is real and is the app's rendered
/// "retry" vocabulary.
public enum CommentRead: Sendable, Equatable {
    /// The log's comments, in append order (oldest first). Empty when the
    /// ticket has no log — the common case, and one that draws no section.
    case list([Comment])
    /// The store was locked for longer than the read waited. The log is fine;
    /// the read can be tried again. A WAL snapshot reads past a normal writer;
    /// this is the database held exclusively, not a damaged line.
    case busy(String)
    /// The log could not be read — a damaged line, most often. The message is
    /// the core's own, and it names the file to repair.
    case failed(String)
}

extension Store {

    /// Reads a ticket's comment log and says how the read ended.
    ///
    /// This is the call the app's ticket sheet makes. The read is a WAL
    /// snapshot, so a CLI append landing in the same window does not block it;
    /// a database held exclusively does, and the store's ~2 s window ends that
    /// in a timeout — the store being busy, not the log being damaged. The two
    /// failures get their own words, because they ask the human for different
    /// things: one asks for a retry, the other for a repair.
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
