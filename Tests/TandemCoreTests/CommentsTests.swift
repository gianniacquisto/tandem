import Darwin
import Foundation
import Testing
@testable import TandemCore

/// The comments sidecar (ADR 0017, amendment 2026-10-20): `append` / `list`
/// over `<home>/comments/<ticket-id>.jsonl`, one JSON line per comment,
/// appended under the store's own flock. The log is append-only: no edit, no
/// delete, and a deleted ticket's log stays on disk.
@Suite("Comments sidecar")
struct CommentsTests {

    private func tempStore() throws -> (Store, URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-comments-tests-\(UUID().uuidString)")
        return (Store(directory: dir), dir)
    }

    private func makeTicket(_ store: Store, _ title: String = "A ticket") throws -> Ticket {
        try store.add(title).value
    }

    private func logFile(_ store: Store, _ id: UUID) -> URL {
        store.commentsDirectory.appendingPathComponent("\(id.uuidString).jsonl")
    }

    @Test("the first append creates the directory and the log, and list reads it back")
    func firstAppendCreatesEverything() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)

        let fm = FileManager.default
        #expect(!fm.fileExists(atPath: store.commentsDirectory.path))

        let result = try store.append(ticketID: ticket.id.uuidString,
                                      author: "builder", body: "did you mean X?")
        let comment = result.value
        #expect(comment.author == "builder")
        #expect(comment.body == "did you mean X?")
        #expect(result.note == nil)

        var isDir: ObjCBool = false
        #expect(fm.fileExists(atPath: store.commentsDirectory.path, isDirectory: &isDir) && isDir.boolValue)
        let raw = try String(contentsOf: logFile(store, ticket.id), encoding: .utf8)
        #expect(raw.hasSuffix("\n"))
        #expect(raw.split(separator: "\n").count == 1) // one line, one trailing newline

        let listed = try store.list(ticketID: ticket.id.uuidString).value
        #expect(listed.count == 1)
        #expect(listed.first == comment)
        // `at` is ISO-8601 UTC with a trailing Z, like every store timestamp.
        #expect(Timestamp.encode(comment.at).hasSuffix("Z"))
    }

    @Test("comments read back in append order")
    func appendOrderIsReadOrder() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)
        for body in ["first", "second", "third"] {
            _ = try store.append(ticketID: ticket.id.uuidString, author: "builder", body: body)
        }
        let listed = try store.list(ticketID: ticket.id.uuidString).value
        #expect(listed.map(\.body) == ["first", "second", "third"])
    }

    @Test("an empty or whitespace-only body is rejected, and nothing is written")
    func emptyBodyRejected() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)

        for body in ["", "   ", "\n  \n"] {
            do {
                _ = try store.append(ticketID: ticket.id.uuidString, author: "builder", body: body)
                Issue.record("expected emptyCommentBody for body \(body.debugDescription)")
            } catch DomainError.emptyCommentBody {
                // The domain failure, not a usage one.
            }
        }
        #expect(!FileManager.default.fileExists(
            atPath: logFile(store, ticket.id).path))
        #expect(try store.list(ticketID: ticket.id.uuidString).value.isEmpty)
    }

    @Test("an unknown id on append fails with the open list, like complete")
    func unknownIDOnAppend() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)
        let unknown = "00000000-0000-0000-0000-000000000000"

        do {
            _ = try store.append(ticketID: unknown, author: "builder", body: "hello")
            Issue.record("expected unknownID")
        } catch let DomainError.unknownID(id, open) {
            #expect(id == unknown)
            #expect(open.map(\.id) == [ticket.id])
        }
        // A non-UUID id is unknown too.
        do {
            _ = try store.append(ticketID: "not a uuid", author: "builder", body: "hello")
            Issue.record("expected unknownID")
        } catch is DomainError {
        }
        #expect(!FileManager.default.fileExists(
            atPath: store.commentsDirectory.path))
    }

    @Test("an unknown id on list is an empty list, not an error")
    func unknownIDOnListIsEmpty() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        _ = try makeTicket(store)

        #expect(try store.list(ticketID: "00000000-0000-0000-0000-000000000000").value.isEmpty)
        #expect(try store.list(ticketID: "not a uuid").value.isEmpty)
    }

    @Test("a comment survives an edit of the ticket")
    func survivesTicketEdits() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)
        _ = try store.append(ticketID: ticket.id.uuidString,
                             author: "builder", body: "asked mid-build")

        _ = try store.set(ticket.id.uuidString, title: .set("A renamed ticket"))
        _ = try store.complete(ticket.id.uuidString)

        let listed = try store.list(ticketID: ticket.id.uuidString).value
        #expect(listed.map(\.body) == ["asked mid-build"])
    }

    @Test("the author is trimmed; an absent author is the empty string")
    func authorIsTrimmed() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)

        let trimmed = try store.append(ticketID: ticket.id.uuidString,
                                       author: "  builder  ", body: "signed").value
        #expect(trimmed.author == "builder")
        let unnamed = try store.append(ticketID: ticket.id.uuidString,
                                       author: "", body: "anonymous").value
        #expect(unnamed.author == "")
    }

    @Test("the body is trimmed at its edges, kept whole inside")
    func bodyIsTrimmed() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)

        let comment = try store.append(ticketID: ticket.id.uuidString,
                                       author: "builder",
                                       body: "\n  did you mean X?\n\n").value
        #expect(comment.body == "did you mean X?")
    }

    @Test("a lowercase and an uppercase id reach the same log")
    func idCaseIsCanonical() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)
        _ = try store.append(ticketID: ticket.id.uuidString, author: "builder", body: "hi")

        let listed = try store.list(ticketID: ticket.id.uuidString.lowercased()).value
        #expect(listed.map(\.body) == ["hi"])
        #expect(try FileManager.default.contentsOfDirectory(
            atPath: store.commentsDirectory.path).count == 1)
    }

    @Test("a damaged line is a loud failure, not a silent skip")
    func damagedLineFails() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)
        _ = try store.append(ticketID: ticket.id.uuidString, author: "builder", body: "ok")

        let url = logFile(store, ticket.id)
        try Data("{not a comment\n".utf8).append(to: url)
        do {
            _ = try store.list(ticketID: ticket.id.uuidString)
            Issue.record("expected a damaged-log failure")
        } catch let StoreIOError.io(detail) {
            #expect(detail.contains("damaged line"))
            #expect(detail.contains("damaged") && detail.contains(url.lastPathComponent))
        }
    }

}

/// The app's read path (`Sources/Tandem/Model.swift` → `TicketSheet.swift`):
/// `Store.commentRead(ticketID:)` is the only call the GUI makes on the sidecar,
/// and it is the whole of what the ticket sheet draws — the log, or the reason it
/// could not be read. The GUI never writes the log — that is ticket `3A44B7FD`.
///
/// The mapping lives in the core (`Sources/TandemCore/CommentRead.swift`), so
/// this suite reaches the mapping the app actually uses. The app has no test
/// target: what this suite cannot reach is the sheet's *drawing* of each case.
@Suite("The app's read path")
struct CommentReadPathTests {

    private func tempStore() throws -> (Store, URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-comment-read-tests-\(UUID().uuidString)")
        return (Store(directory: dir), dir)
    }

    private func makeTicket(_ store: Store) throws -> Ticket {
        try store.add("A ticket").value
    }

    private func logFile(_ store: Store, _ id: UUID) -> URL {
        store.commentsDirectory.appendingPathComponent("\(id.uuidString).jsonl")
    }

    @Test("the app reads the log in append order — oldest first — with author, time and body")
    func readsAppendOrder() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)

        for (author, body) in [("builder", "first"), ("human", "second"), ("", "third")] {
            _ = try store.append(ticketID: ticket.id.uuidString, author: author, body: body)
        }

        // The call the app makes, and only this call.
        let read = store.commentRead(ticketID: ticket.id.uuidString)
        guard case .list(let listed) = read else {
            Issue.record("expected the log, got \(read)")
            return
        }
        #expect(listed.map(\.body) == ["first", "second", "third"])
        #expect(listed.map(\.author) == ["builder", "human", ""])
        // The times never go backwards: the order the sheet draws is the order the
        // log was written in.
        #expect(listed.map(\.at) == listed.map(\.at).sorted())
    }

    @Test("a ticket with no log reads as no comments, and the read creates nothing")
    func noLogReadsEmptyAndWritesNothing() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)

        let read = store.commentRead(ticketID: ticket.id.uuidString)
        guard case .list(let listed) = read else {
            Issue.record("expected an empty log, got \(read)")
            return
        }
        #expect(listed.isEmpty)
        // A read never creates the sidecar: the directory is made on the first
        // append, so a reader that made it would leave a ticket looking discussed.
        #expect(!FileManager.default.fileExists(atPath: store.commentsDirectory.path))
    }

    @Test("a damaged line is a failure for the sheet to show, not an empty list")
    func damagedLineIsAFailure() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)
        _ = try store.append(ticketID: ticket.id.uuidString, author: "builder", body: "ok")
        try Data("{not a comment\n".utf8).append(to: logFile(store, ticket.id))

        let read = store.commentRead(ticketID: ticket.id.uuidString)
        guard case .failed(let message) = read else {
            Issue.record("expected a damaged-log failure, got \(read)")
            return
        }
        // The sheet shows this message where the conversation would have been.
        #expect(message.contains("damaged line"))
        #expect(message.contains(logFile(store, ticket.id).lastPathComponent))
        // A damaged log is not a busy store: the two ask the human for different
        // things, and only the busy one is worth a retry.
        #expect(!message.contains("locked by another process"))
    }

    @Test("a lock timeout is busy, not a damaged log")
    func lockTimeoutIsBusyNotDamaged() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)
        _ = try store.append(ticketID: ticket.id.uuidString, author: "builder", body: "ok")

        // Hold the store's own lock, the way a CLI append does: the read then
        // waits out the ~2 s window and gives up on the lock.
        let fd = open(store.lockURL.path, O_RDWR | O_CREAT, 0o644)
        #expect(fd >= 0)
        #expect(flock(fd, LOCK_EX) == 0)
        defer { close(fd) }

        let read = store.commentRead(ticketID: ticket.id.uuidString)
        guard case .busy(let message) = read else {
            Issue.record("expected a busy read, got \(read)")
            return
        }
        // The words say the store is locked, and ask for a retry.
        #expect(message.contains("locked by another process"))
        #expect(message.lowercased().contains("retry"))
        // Nothing about the log is wrong, so nothing asks for a repair: a busy
        // store must not read as a damaged file.
        #expect(!message.contains("damaged"))
        #expect(!message.contains("repair"))
        #expect(!message.contains(logFile(store, ticket.id).lastPathComponent))
    }
}

/// The app's write path (`Sources/Tandem/Model.swift` → `Store.commentWrite`):
/// the human's comment is appended to the sidecar, with the human as the author,
/// and the write hands back the record it wrote. This is the call the ticket
/// sheet's composer makes — the same sidecar the CLI's `todo comment add` writes.
///
/// The path lives in the core (`Sources/TandemCore/CommentWrite.swift`), so this
/// suite reaches the path the app actually uses. The app has no test target:
/// what this suite cannot reach is the sheet's drawing of it.
@Suite("The app's write path")
struct CommentWritePathTests {

    private func tempStore() throws -> (Store, URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-comment-write-tests-\(UUID().uuidString)")
        return (Store(directory: dir), dir)
    }

    private func makeTicket(_ store: Store) throws -> Ticket {
        try store.add("A ticket").value
    }

    private func logFile(_ store: Store, _ id: UUID) -> URL {
        store.commentsDirectory.appendingPathComponent("\(id.uuidString).jsonl")
    }

    @Test("the human's comment lands in the sidecar, and the write hands back the record")
    func theHumansCommentLands() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)

        // The call the app makes, and only this call.
        let written = try store.commentWrite(ticketID: ticket.id.uuidString,
                                             body: "the human's own note")
        #expect(written.note == nil)

        // The record that was written: the human is the author, and the body is
        // the human's own text.
        #expect(written.value.author == "human")
        #expect(written.value.body == "the human's own note")

        // The sidecar is the one log: the file `todo comment list` reads.
        let raw = try String(contentsOf: logFile(store, ticket.id), encoding: .utf8)
        #expect(raw.hasSuffix("\n"))
        #expect(raw.split(separator: "\n").count == 1) // one line, one trailing newline
        #expect(raw.contains("\"author\":\"human\""))
        #expect(raw.contains("the human's own note"))

        // The app's read: the comment is in the list at once.
        guard case .list(let listed) = store.commentRead(ticketID: ticket.id.uuidString) else {
            Issue.record("expected the log")
            return
        }
        #expect(listed.map(\.body) == ["the human's own note"])
        #expect(listed.map(\.author) == ["human"])
        // And the record the write handed back is the one the log holds.
        #expect(listed.first?.at == written.value.at)

        // The CLI's read: the same comment, out of the same log.
        let cli = try store.list(ticketID: ticket.id.uuidString).value
        #expect(cli.map(\.body) == ["the human's own note"])
        #expect(cli.map(\.author) == ["human"])
    }

    @Test("an empty body is refused with the core's words, and nothing is written")
    func emptyBodyIsRefused() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)

        for body in ["", "   ", "\n  \n"] {
            do {
                _ = try store.commentWrite(ticketID: ticket.id.uuidString, body: body)
                Issue.record("expected emptyCommentBody for body \(body.debugDescription)")
            } catch let error as DomainError {
                // The domain failure, not a usage one — the same one the CLI
                // refuses an empty body with.
                #expect(error == .emptyCommentBody)
                // The message the app shows where the composer was.
                #expect(error.message.contains("empty"))
            }
        }
        // Nothing was written: not the log, and not even its directory — a
        // refusal never leaves a ticket looking discussed.
        #expect(!FileManager.default.fileExists(atPath: store.commentsDirectory.path))
        #expect(try store.list(ticketID: ticket.id.uuidString).value.isEmpty)
    }

    @Test("the human's comment joins the log in order, after the agent's")
    func theHumansCommentJoinsTheLogInOrder() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)

        // An agent's comment first, written the CLI's way.
        _ = try store.append(ticketID: ticket.id.uuidString, author: "builder", body: "did you mean X?")
        // Then the human's, written the app's way.
        let written = try store.commentWrite(ticketID: ticket.id.uuidString, body: "yes, X it is")
        #expect(written.value.body == "yes, X it is")
        #expect(written.value.author == "human")

        // The order is the log's order: the app's comment is last.
        let listed = try store.list(ticketID: ticket.id.uuidString).value
        #expect(listed.map(\.body) == ["did you mean X?", "yes, X it is"])
        #expect(listed.map(\.author) == ["builder", "human"])
    }

    @Test("the write path takes the lock once — the record comes back, not a busy store")
    func theWritePathTakesTheLockOnce() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)
        let log = logFile(store, ticket.id)

        // Another holder of the store's own lock — a CLI append, or the app's own
        // read path. It holds the lock with its own fd, which contends through
        // the same flock as another process's would (the repo's own convention:
        // `ConcurrencyTests`).
        //
        // It holds the lock long enough for the write path to queue behind it,
        // then lets the write in, and takes the lock again the moment the
        // write's log file appears — which is while the append still holds it,
        // so the second holder is queued first and is handed the lock the
        // moment the append releases it. A *second* lock acquisition by the
        // write path would then wait out the whole window and come back
        // "busy" — for a comment that was written.
        let holder = DispatchGroup()
        let hasLock = DispatchSemaphore(value: 0)
        holder.enter()
        DispatchQueue.global().async {
            defer { holder.leave() }
            let fd = open(store.lockURL.path, O_RDWR | O_CREAT, 0o644)
            guard fd >= 0 else { hasLock.signal(); return }
            guard flock(fd, LOCK_EX) == 0 else { close(fd); hasLock.signal(); return }
            hasLock.signal()
            usleep(300_000)                 // 300 ms: the write path waits here
            close(fd)                       // and comes in
            // The write's log file appears: the append has created it, and it
            // still holds the lock. Take the lock again, and keep it.
            let deadline = Date().addingTimeInterval(10)
            while !FileManager.default.fileExists(atPath: log.path), Date() < deadline {
                usleep(50)
            }
            guard FileManager.default.fileExists(atPath: log.path) else { return }
            let second = open(store.lockURL.path, O_RDWR | O_CREAT, 0o644)
            guard second >= 0 else { return }
            _ = flock(second, LOCK_EX)      // granted when the append releases
            usleep(2_500_000)               // the full wait window
            close(second)
        }
        #expect(hasLock.wait(timeout: .now() + 15) == .success, "the holder never took the lock")

        // The write path: one lock, one answer. The record that was written
        // comes back — not "busy", and not a lost draft.
        let written = try store.commentWrite(ticketID: ticket.id.uuidString,
                                             body: "the human's own note")
        #expect(written.value.author == "human")
        #expect(written.value.body == "the human's own note")

        // The comment is in the log: nothing was lost. (The log is read from
        // the file — the store's lock is held by the holder for the whole
        // window, and the read path is the one that waits it out.)
        let raw = try String(contentsOf: log, encoding: .utf8)
        #expect(raw.contains("\"author\":\"human\""))
        #expect(raw.contains("the human's own note"))

        // And the read path, under the same contention, still says busy —
        // which is the answer the write path no longer gives.
        let read = store.commentRead(ticketID: ticket.id.uuidString)
        guard case .busy(let message) = read else {
            Issue.record("expected a busy read, got \(read)")
            return
        }
        #expect(message.contains("locked by another process"))
        #expect(message.lowercased().contains("retry"))
    }

    @Test("the CLI's `todo comment list` shows the comment the app wrote — one log")
    func theCLIReadsWhatTheAppWrote() throws {
        let home = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-comment-cli-tests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: home) }
        let cli = CLIHarness()

        // A ticket, filed through the CLI.
        let id = try cli.addTicket(home: home, title: "A ticket")
        #expect(!id.isEmpty)

        // The app's write path, against the same home.
        let store = Store(directory: home)
        _ = try store.commentWrite(ticketID: id, body: "the human's own note")

        // The CLI's read: the same comment, out of the same sidecar.
        let comments = try cli.comments(home: home, id: id)
        #expect(comments.count == 1)
        #expect(comments.first?["body"] as? String == "the human's own note")
        #expect(comments.first?["author"] as? String == "human")
    }
}

/// The sheet's **refresh** (`Sources/Tandem/TicketSheet.swift` →
/// `Model.comments(for:)` → `Store.commentRead(ticketID:)`): the read that is
/// not the write's read-back.
///
/// The write path takes the store's lock once and hands back the record it
/// wrote, so the sheet's list is the log as of opening plus what that sheet
/// wrote — a comment another actor appended in between stays invisible until
/// the sheet reads the log again. This suite reaches the core behaviour that
/// refresh is built on: a read of its own, which sees another actor's comment,
/// and which takes nothing away when it does not happen.
///
/// The sheet's *drawing* of each case is the human's visual pass (the app has
/// no test target); what this suite cannot reach is what the sheet does with
/// the answer.
@Suite("The sheet's refresh")
struct CommentRefreshTests {

    private func tempStore() throws -> (Store, URL) {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-comment-refresh-tests-\(UUID().uuidString)")
        return (Store(directory: dir), dir)
    }

    private func makeTicket(_ store: Store) throws -> Ticket {
        try store.add("A ticket").value
    }

    private func logFile(_ store: Store, _ id: UUID) -> URL {
        store.commentsDirectory.appendingPathComponent("\(id.uuidString).jsonl")
    }

    @Test("the refresh sees a comment another actor appended while the sheet was open")
    func theRefreshSeesAnotherActorsComment() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)

        // The sheet opens: the log as it reads now — nobody has said anything.
        guard case .list(let opened) = store.commentRead(ticketID: ticket.id.uuidString) else {
            Issue.record("expected the log")
            return
        }
        #expect(opened.isEmpty)

        // The human posts, through the app's write path — one lock, one answer.
        let written = try store.commentWrite(ticketID: ticket.id.uuidString,
                                             body: "the human's own note")
        // And an agent comments from the terminal, through the CLI's own path:
        // the same file, the same log.
        _ = try store.append(ticketID: ticket.id.uuidString, author: "builder", body: "did you mean X?")

        // The refresh: a read of its own, with no write of its own to hang on.
        guard case .list(let refreshed) = store.commentRead(ticketID: ticket.id.uuidString) else {
            Issue.record("expected the log")
            return
        }
        // The agent's comment is in the list — it was invisible until the sheet
        // read the log again — and the human's own is in it too, in the order
        // the sidecar holds.
        #expect(refreshed.map(\.body) == ["the human's own note", "did you mean X?"])
        #expect(refreshed.map(\.author) == ["human", "builder"])
        // The human's own comment is the record the write handed back — the
        // same one, in the log.
        #expect(refreshed.first?.at == written.value.at)
    }

    @Test("a refresh that answers busy leaves the log — and so the sheet's list — as it was")
    func aBusyRefreshTakesNothingAway() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)
        let log = logFile(store, ticket.id)

        // The human posts, and the sheet's list has that comment in it.
        let written = try store.commentWrite(ticketID: ticket.id.uuidString,
                                             body: "the human's own note")
        let before = try String(contentsOf: log, encoding: .utf8)

        // Another process holds the store's lock — a CLI write, say. The
        // refresh waits out the window and gives up.
        let fd = open(store.lockURL.path, O_RDWR | O_CREAT, 0o644)
        #expect(fd >= 0)
        #expect(flock(fd, LOCK_EX) == 0)
        defer { close(fd) }

        let read = store.commentRead(ticketID: ticket.id.uuidString)
        guard case .busy(let message) = read else {
            Issue.record("expected a busy read, got \(read)")
            return
        }
        // The words ask for a retry, not a repair: nothing is wrong with the log.
        #expect(message.contains("locked by another process"))
        #expect(message.lowercased().contains("retry"))
        #expect(!message.contains("damaged"))

        // The refresh took nothing away: the log is byte for byte what it was,
        // so the sheet's list — which holds the human's comment — stays as it
        // was, and the comment the human posted is still in it.
        #expect(try String(contentsOf: log, encoding: .utf8) == before)
        #expect(before.contains("\"author\":\"human\""))
        #expect(before.contains("the human's own note"))
        // And the record the write handed back is still the one the log holds.
        #expect(written.value.body == "the human's own note")
    }
}

/// Append to an existing file (the test's hand-edit of the log).
private extension Data {
    func append(to url: URL) throws {
        var existing = Data()
        if FileManager.default.fileExists(atPath: url.path) {
            existing = try Data(contentsOf: url)
        }
        existing.append(self)
        try existing.write(to: url)
    }
}
