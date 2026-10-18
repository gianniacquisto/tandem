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
// semantics (`list(rawFilter:)`, `add`, `complete`, `rename`) and the encoding
// (`StoreEncoding`, `tandemErrorText`) stay in TandemCore, which the single test
// seam (`TandemCoreTests`) covers.
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
    case .todoList(let filter):
        return run {
            let result = try Store().list(rawFilter: filter)
            return try StoreEncoding.listResult(result.value, note: result.note)
        }
    case .todoAdd(let title):
        return run {
            let result = try Store().add(title)
            return try StoreEncoding.todoResult(result.value, note: result.note)
        }
    case .todoComplete(let id):
        return run {
            let result = try Store().complete(id)
            return try StoreEncoding.todoResult(result.value, note: result.note)
        }
    case .noteRename(let from, let to):
        return run {
            let result = try Bikeshed().rename(from: from, to: to)
            return try renameResult(result.value)
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

/// The `note rename` result — `{"path": "<new>"}` — serialized in the same style
/// as `StoreEncoding` (sorted keys, UTF-8). A note operation has no store-schema
/// encoder of its own, so the CLI builds the one-value object here.
func renameResult(_ newPath: String) throws -> String {
    let data = try JSONSerialization.data(
        withJSONObject: ["path": newPath], options: [.sortedKeys])
    guard let text = String(data: data, encoding: .utf8) else {
        throw StoreIOError.io("could not serialize result to UTF-8")
    }
    return text
}
