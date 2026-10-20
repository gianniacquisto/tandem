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

    private func makeTicket(_ store: Store, _ title: String = "A ticket") throws -> Todo {
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
                                      author: "blue", body: "did you mean X?")
        let comment = result.value
        #expect(comment.author == "blue")
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
            _ = try store.append(ticketID: ticket.id.uuidString, author: "blue", body: body)
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
                _ = try store.append(ticketID: ticket.id.uuidString, author: "blue", body: body)
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
            _ = try store.append(ticketID: unknown, author: "blue", body: "hello")
            Issue.record("expected unknownID")
        } catch let DomainError.unknownID(id, open) {
            #expect(id == unknown)
            #expect(open.map(\.id) == [ticket.id])
        }
        // A non-UUID id is unknown too.
        do {
            _ = try store.append(ticketID: "not a uuid", author: "blue", body: "hello")
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
                             author: "blue", body: "asked mid-build")

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
                                       author: "  blue  ", body: "signed").value
        #expect(trimmed.author == "blue")
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
                                       author: "blue",
                                       body: "\n  did you mean X?\n\n").value
        #expect(comment.body == "did you mean X?")
    }

    @Test("a lowercase and an uppercase id reach the same log")
    func idCaseIsCanonical() throws {
        let (store, dir) = try tempStore()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ticket = try makeTicket(store)
        _ = try store.append(ticketID: ticket.id.uuidString, author: "blue", body: "hi")

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
        _ = try store.append(ticketID: ticket.id.uuidString, author: "blue", body: "ok")

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
