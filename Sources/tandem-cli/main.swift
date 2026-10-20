import Foundation
import TandemCore

// tandem-cli — Tandem's agent face (ADR 0009).
//
// A synchronous, single-shot command line over the shared store and bikeshed: it
// links TandemCore, writes the same files the app does under the same sidecar
// locks, and dies with the invocation — so an upgrade can never leave a stale
// writer behind (the failure the MCP server it replaces made possible). It
// starts no GUI and needs no running app.
//
// This file is the whole adapter: it turns a command line into the core calls
// and writes their outcome to the right stream. It adds no boundary logic — the
// semantics (`list`, `TodoQuery.parse`, `add`, `set`, `move`, `complete`,
// `archiveDone`, `rename`, `capture`, `stats`) and the encoding (`StoreEncoding`,
// `tandemErrorText`) stay in TandemCore, which the single test seam
// (`TandemCoreTests`) covers. The two things that belong to a process rather
// than to the domain live here: stdin and the environment.
//
// Streams and exit codes (the contract):
// - success → exactly one JSON object on stdout, one trailing newline, exit 0;
// - a domain failure → the short actionable text on stderr, exit 1;
// - a usage failure (no arguments, an unknown subcommand, an unknown flag) →
//   the usage text on stderr, exit 2. A *missing flag value* is not a usage
//   failure: it becomes the empty string and the domain rejects it, exactly as
//   the MCP server read an absent argument as "".

exit(dispatch(Array(CommandLine.arguments.dropFirst())))

// MARK: - Dispatch

/// Route one parsed command to the core and return the process exit code.
/// `--help` prints the usage to stdout and exits 0; a parse failure prints it to
/// stderr and exits 2.
func dispatch(_ args: [String]) -> Int32 {
    let command: Command
    do {
        command = try parseCommand(args)
    } catch {
        FileHandle.standardError.write(Data((Usage.text + "\n").utf8))
        return 2
    }
    switch command {
    case .help:
        print(Usage.text)
        return 0
    case .doctor:
        return runDoctor()
    case .todoList(let filter, let text, let status, let assignee, let lane):
        return run {
            let query = try TodoQuery.parse(
                filter: filter,
                text: text,
                status: status,
                assignee: try resolveAssignee(assignee),
                lane: lane)
            let result = try Store().list(query)
            return try StoreEncoding.listResult(result.value, note: result.note)
        }
    case .todoAdd(let title, let status, let assignee, let lane):
        return run {
            let result = try Store().add(
                title,
                lane: lane,
                status: try status.map { try Status.parse(raw: $0) } ?? .backlog,
                assignee: try resolveAssignee(assignee))
            return try StoreEncoding.todoResult(result.value, note: result.note)
        }
    case .todoSet(let id, let title, let status, let assignee, let description):
        return run {
            // `--description -`: the body is stdin, the precedent `note capture` set.
            let body: Edit<String?>
            if let description {
                body = .set(description == "-" ? try readStandardInput() : description)
            } else {
                body = .unchanged
            }
            let result = try Store().set(
                id,
                title: edit(title) { $0 },
                status: try edit(status) { try Status.parse(raw: $0) },
                assignee: assignee.map { .set(try resolveAssignee($0)) } ?? .unchanged,
                description: body)
            return try StoreEncoding.todoResult(result.value, note: result.note)
        }
    case .todoMove(let id, let lane, let index):
        return run {
            let result = try Store().move(id, toLane: lane, at: try parseIndex(index))
            return try StoreEncoding.todoResult(result.value, note: result.note)
        }
    case .todoComplete(let id):
        return run {
            let result = try Store().complete(id)
            return try StoreEncoding.todoResult(result.value, note: result.note)
        }
    case .todoArchive:
        return run {
            let result = try Store().archiveDone()
            return try StoreEncoding.archivedResult(result.value, note: result.note)
        }
    case .noteRename(let from, let to):
        return run {
            let result = try Bikeshed().rename(from: from, to: to)
            return try pathResult(result.value)
        }
    case .noteCapture(let kind, let project, let session):
        return run {
            guard let captureKind = CaptureKind(rawValue: kind) else {
                throw BikeshedError.invalidCaptureKind(kind: kind)
            }
            let result = try Bikeshed().capture(
                project: project ?? captureProjectDefault(),
                kind: captureKind,
                body: readStandardInput(),
                session: session ?? captureSessionDefault())
            return try pathResult(result.value)
        }
    case .noteStats(let project):
        return run {
            let result = try Bikeshed().stats(project: project)
            return try StoreEncoding.bikeshedStatsResult(result.value)
        }
    }
}

/// Run one core call, then write its outcome: the encoded JSON plus a newline on
/// stdout and exit 0, or `tandemErrorText(error)` plus a newline on stderr and
/// exit 1. Nothing else ever touches stdout.
func run(_ body: () throws -> String) -> Int32 {
    do {
        print(try body())
        return 0
    } catch {
        FileHandle.standardError.write(Data((tandemErrorText(error) + "\n").utf8))
        return 1
    }
}

/// `doctor` (ADR 0014): a setup report is not a domain call, so it does not go
/// through `run`. It always prints the report JSON to stdout — the report is the
/// success value, even when a check fails — and exits 0 when every
/// failing-capable check passed, 1 otherwise, echoing each failing check's
/// next step to stderr. A serialization failure is the only other exit 1.
func runDoctor() -> Int32 {
    let report = SetupDiagnostics.run()
    do {
        print(try StoreEncoding.doctorResult(report))
    } catch {
        FileHandle.standardError.write(Data((tandemErrorText(error) + "\n").utf8))
        return 1
    }
    guard report.ok else {
        for check in report.checks where !check.ok {
            FileHandle.standardError.write(Data(("doctor: \(check.name) — \(check.detail)\n").utf8))
        }
        return 1
    }
    return 0
}

/// A raw flag as an `Edit`: absent is `.unchanged`, present is `.set` of the converted
/// value — the CLI spelling of "leave it alone" versus "set it".
func edit<T: Sendable>(_ raw: String?, _ convert: (String) throws -> T) rethrows -> Edit<T> {
    guard let raw else { return .unchanged }
    return .set(try convert(raw))
}

/// `move`'s position: absent means the end of the target lane, which appends without
/// disturbing the order above it; a value is the 0-based index within that lane.
func parseIndex(_ raw: String?) throws -> Int {
    guard let raw else { return Int.max }
    guard let index = Int(raw) else { throw DomainError.invalidIndex(raw) }
    return index
}

/// `--assignee self` means the agent running this command: its name is `$TANDEM_AGENT`,
/// the one place a ticket owner can come from until the persona registry exists. Any
/// other value passes straight through to the core — including `""`, which selects the
/// unowned tickets, and so must not be mistaken for "me".
func resolveAssignee(_ raw: String?) throws -> String? {
    guard raw == "self" else { return raw }
    guard let name = ProcessInfo.processInfo.environment["TANDEM_AGENT"],
          !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    else {
        throw DomainError.agentNameUnset
    }
    return name
}

/// The body of a `note capture` call: everything on stdin, decoded as UTF-8,
/// empty allowed (ADR 0016). Invalid UTF-8 is a domain failure, not a usage one.
func readStandardInput() throws -> String {
    let data = FileHandle.standardInput.readDataToEndOfFile()
    guard let text = String(data: data, encoding: .utf8) else {
        throw BikeshedIOError.io("stdin is not valid UTF-8")
    }
    return text
}

/// The `note capture` project default: the working directory's last path
/// component (`…/projects/tandem` → `tandem`), per ADR 0016 — the codebase being
/// worked on, so its capture layer nests under its own folder.
func captureProjectDefault() -> String {
    URL(fileURLWithPath: FileManager.default.currentDirectoryPath).lastPathComponent
}

/// The `note capture` session default: `$OPENCODE_SESSION_ID` when set to a
/// non-empty value, else a short generated token for hosts that export none
/// (ADR 0016).
func captureSessionDefault() -> String {
    if let session = ProcessInfo.processInfo.environment["OPENCODE_SESSION_ID"],
       !session.isEmpty
    {
        return session
    }
    return String(UUID().uuidString.prefix(8)).lowercased()
}

/// The `note rename` / `note capture` result — `{"path": "<path>"}` — serialized
/// in the same style as `StoreEncoding` (sorted keys, UTF-8). A note operation
/// has no store-schema encoder of its own, so the CLI builds the one-value
/// object here.
func pathResult(_ path: String) throws -> String {
    let data = try JSONSerialization.data(
        withJSONObject: ["path": path], options: [.sortedKeys])
    guard let text = String(data: data, encoding: .utf8) else {
        throw StoreIOError.io("could not serialize result to UTF-8")
    }
    return text
}
