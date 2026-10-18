import Foundation

/// The parsed command line: one of the five verbs, or a request for the usage
/// text. Long flags only, and the flags mirror the core argument names exactly
/// (`--filter`, `--title`, `--id`, `--from`, `--to`) — the CLI renames nothing.
/// `doctor` is a setup/diagnostic verb outside the `todo`/`note` domains
/// (ADR 0014) and takes no flags.
enum Command {
    case help
    case doctor
    case todoList(filter: String?)
    case todoAdd(title: String)
    case todoComplete(id: String)
    case noteRename(from: String, to: String)
}

/// A usage failure — no arguments, an unknown subcommand, an unknown flag, or a
/// stray token. It carries no message: the caller prints the usage text to
/// stderr and exits 2. A *missing flag value* is deliberately not one of these
/// (see `parseFlags`).
struct UsageError: Error {}

/// Parse the arguments after the program name. `--help` / `-h` as the first
/// argument is the help request; otherwise the first argument names the group
/// (`todo` / `note`), the second the verb, and the rest are `--flag value`
/// pairs validated against that verb.
func parseCommand(_ args: [String]) throws -> Command {
    guard let first = args.first else { throw UsageError() }
    switch first {
    case "--help", "-h":
        return .help
    case "doctor":
        // A setup/diagnostic verb (ADR 0014): no group, no flags. Any extra
        // argument is a usage failure.
        guard args.count == 1 else { throw UsageError() }
        return .doctor
    case "todo":
        return try parseTodo(Array(args.dropFirst()))
    case "note":
        return try parseNote(Array(args.dropFirst()))
    default:
        throw UsageError()
    }
}

// MARK: - todo

/// `todo list|add|complete` — the three todo verbs (the agent surface).
private func parseTodo(_ args: [String]) throws -> Command {
    guard let verb = args.first else { throw UsageError() }
    let flags = try parseFlags(Array(args.dropFirst()))
    switch verb {
    case "list":
        try accept(flags, ["--filter"])
        // No `--filter` is `nil` — the core's default (`open`); the CLI does not
        // re-derive the parse, `list(rawFilter:)` owns it.
        return .todoList(filter: flags["--filter"])
    case "add":
        try accept(flags, ["--title"])
        // A missing `--title` is not a usage error: it is the empty title the
        // domain rejects (`emptyTitle`), exactly as the MCP server read an
        // absent argument.
        return .todoAdd(title: flags["--title"] ?? "")
    case "complete":
        try accept(flags, ["--id"])
        // A missing `--id` is likewise the empty id whose `unknownID` error
        // carries the open list.
        return .todoComplete(id: flags["--id"] ?? "")
    default:
        throw UsageError()
    }
}

// MARK: - note

/// `note rename` — the one note verb (ADR 0009): the rename whose wikilink
/// rewriting a raw `mv` cannot reproduce.
private func parseNote(_ args: [String]) throws -> Command {
    guard let verb = args.first else { throw UsageError() }
    let flags = try parseFlags(Array(args.dropFirst()))
    switch verb {
    case "rename":
        try accept(flags, ["--from", "--to"])
        return .noteRename(from: flags["--from"] ?? "", to: flags["--to"] ?? "")
    default:
        throw UsageError()
    }
}

// MARK: - Flags

/// Parse `tokens` as `--flag value` pairs. A flag's value is the next token
/// when one exists and `""` when the flag is last: a missing value is the
/// domain's to reject, not a usage error — the retired MCP server read an absent
/// argument the same way. An unknown flag or a bare token where a flag is
/// expected is a usage error; a repeated flag keeps its last value.
private func parseFlags(_ tokens: [String]) throws -> [String: String] {
    var values: [String: String] = [:]
    var i = 0
    while i < tokens.count {
        let flag = tokens[i]
        guard flag.hasPrefix("--") else { throw UsageError() }
        if i + 1 < tokens.count {
            values[flag] = tokens[i + 1]
            i += 2
        } else {
            values[flag] = ""
            i += 1
        }
    }
    return values
}

/// Reject any flag this verb does not define (an unknown flag is a usage error).
private func accept(_ values: [String: String], _ allowed: [String]) throws {
    let allowed = Set(allowed)
    guard values.keys.allSatisfy(allowed.contains) else { throw UsageError() }
}
