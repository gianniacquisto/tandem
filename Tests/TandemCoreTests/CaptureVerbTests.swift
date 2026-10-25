import Foundation
import Testing

/// The `note capture` verb at the binary seam: the CLI's face of the junk gate
/// (ticket 824CB789). The core refuses the junk; the verb's contract is the
/// exit codes — a refusal is exit 2 (the caller's text to fix, not a retryable
/// domain failure), and an exact duplicate is a no-op that still exits 0,
/// naming the file already pending.
///
/// Every test runs the binary the way an agent does, against a throwaway home:
/// the CLI is an adapter, and the adapter's contract is the JSON on stdout, the
/// text on stderr and the exit code.
@Suite("The note capture verb")
struct CaptureVerbTests {

    private let cli = CLIHarness()

    // MARK: - The refusals (exit 2)

    @Test("an empty body is refused: exit 2, nothing filed")
    func emptyBodyIsRefused() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let refused = try run(home: home, ["note", "capture", "--kind", "fact", "--project", "tandem"], stdin: "")
        #expect(refused.code == 2)
        #expect(refused.error.contains("Capture body is empty"))
        #expect(!FileManager.default.fileExists(atPath: home.appendingPathComponent("bikeshed/projects").path),
            "a refused capture must not create the bikeshed")
    }

    @Test("a whitespace-only body is refused: exit 2, nothing filed")
    func whitespaceBodyIsRefused() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let refused = try run(home: home, ["note", "capture", "--kind", "fact", "--project", "tandem"], stdin: " \t\n ")
        #expect(refused.code == 2)
        #expect(refused.error.contains("Capture body is empty"))
        #expect(!FileManager.default.fileExists(atPath: home.appendingPathComponent("bikeshed/projects").path))
    }

    @Test("a body with no slug is refused: exit 2 asking for a slug")
    func sluglessBodyIsRefused() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        // The first line is all punctuation — the filename would fall back to
        // the kind's own name (`work-work.md`), the junk the gate exists for.
        let refused = try run(home: home, ["note", "capture", "--kind", "fact", "--project", "tandem"], stdin: "# !!!\n")
        #expect(refused.code == 2)
        #expect(refused.error.contains("no usable slug"))
        #expect(!FileManager.default.fileExists(atPath: home.appendingPathComponent("bikeshed/projects").path))
    }

    // MARK: - The retired kind (exit 1, ticket F75507ED)

    @Test("--kind work is a domain failure pointing at session end --summary")
    func retiredWorkKindIsADomainFailure() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        // A well-formed body: the refusal is about the kind alone.
        let refused = try run(home: home, ["note", "capture", "--kind", "work", "--project", "tandem"], stdin: "# A body\n")
        #expect(refused.code == 1, "a domain failure, not a usage one")
        #expect(refused.error.contains("retired"))
        #expect(refused.error.contains("session end --summary"), "the summary is the work record (ADR 0027)")
        #expect(!FileManager.default.fileExists(atPath: home.appendingPathComponent("bikeshed/projects").path),
            "a refused capture must not create the bikeshed")
    }

    // MARK: - The duplicate no-op (exit 0)

    @Test("a real capture files once; re-filing names the existing capture")
    func duplicateIsANoOp() throws {
        let home = temporaryHome()
        defer { try? FileManager.default.removeItem(at: home) }

        let body = "# x\n\nbody"
        let first = try run(home: home, ["note", "capture", "--kind", "fact", "--project", "tandem"], stdin: body)
        #expect(first.code == 0)
        let firstPath = try cli.object(from: first.output)["path"] as? String
        #expect(firstPath != nil)
        #expect(FileManager.default.fileExists(atPath: home.appendingPathComponent("bikeshed/" + firstPath.unsafelyUnwrapped).path),
            "the first call filed the capture")

        let second = try run(home: home, ["note", "capture", "--kind", "fact", "--project", "tandem"], stdin: body)
        #expect(second.code == 0, "a re-file after a retry is not an error")
        #expect(try cli.object(from: second.output)["path"] as? String == firstPath,
            "the second call names the existing file")
        #expect(try cli.object(from: second.output)["note"] is String,
            "the second call says why nothing new was written")
        #expect(try pendingCaptures(home: home, project: "tandem").count == 1,
            "no second file was written")
    }

    // MARK: - Helpers

    /// A throwaway home: the CLI writes there, never the human's store.
    private func temporaryHome() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("tandem-cli-capture-verb-\(UUID().uuidString)")
    }

    /// Run the CLI with the session id exported, as a host does.
    private func run(home: URL, _ arguments: [String], stdin: String = "") throws -> CLIHarness.Result {
        try cli.run(home: home, arguments, stdin: stdin, environment: ["OPENCODE_SESSION_ID": "ses_test"])
    }

    /// The pending capture files of one project — the files under
    /// `projects/<project>/.raw/**`, the archive under `.done/` excluded.
    private func pendingCaptures(home: URL, project: String) throws -> [String] {
        let raw = home.appendingPathComponent("bikeshed/projects/\(project)/.raw")
        let fm = FileManager.default
        var found: [String] = []
        let entries = (try? fm.contentsOfDirectory(at: raw, includingPropertiesForKeys: nil)) ?? []
        for entry in entries where !entry.lastPathComponent.hasPrefix(".") {
            let files = (try? fm.contentsOfDirectory(at: entry, includingPropertiesForKeys: nil)) ?? []
            found.append(contentsOf: files.map(\.lastPathComponent))
        }
        return found.sorted()
    }
}
