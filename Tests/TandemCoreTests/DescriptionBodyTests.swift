import Foundation
import Testing
@testable import TandemCore

/// The body of a `todo set --description -` write: text, or a domain failure.
/// The store's rule is unchanged — an empty description is a legitimate value,
/// and `--description ""` clears it deliberately — so the refusal belongs to
/// the stdin path alone.
@Suite("Description body")
struct DescriptionBodyTests {

    @Test("an empty body is refused")
    func emptyBodyIsRefused() {
        #expect(throws: DomainError.emptyDescriptionBody) {
            _ = try DescriptionBody.required("")
        }
    }

    @Test("a whitespace-only body is refused")
    func whitespaceBodyIsRefused() {
        for body in ["   ", "\t", "\n", " \t\n "] {
            #expect(throws: DomainError.emptyDescriptionBody) {
                _ = try DescriptionBody.required(body)
            }
        }
    }

    @Test("a body with text is kept, trimmed at its edges")
    func textBodyIsKept() throws {
        #expect(try DescriptionBody.required("  the brief  ") == "the brief")
        #expect(try DescriptionBody.required("line one\nline two") == "line one\nline two")
    }

    @Test("the refusal is a domain failure with actionable text")
    func refusalTextIsActionable() {
        let text = tandemErrorText(DomainError.emptyDescriptionBody)
        #expect(text.contains("empty"))
        #expect(text.contains("--description"))
    }

    @Test("the store still takes an empty description as a deliberate clear")
    func storeStillClearsOnEmpty() throws {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-description-body-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = Store(directory: dir)

        let todo = try store.add("A ticket").value
        _ = try store.set(todo.id.uuidString, description: .set("the brief"))
        #expect(try store.set(todo.id.uuidString, description: .set("")).value.description == nil)
    }
}
