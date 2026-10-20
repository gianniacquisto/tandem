import Foundation

/// The parsed command line: one of the verbs, or a request for the usage text.
/// Long flags only, and the flags mirror the core argument names exactly
/// (`--filter`, `--text`, `--status`, `--assignee`, `--lane`, `--title`, `--id`,
/// `--description`, `--index`, `--from`, `--to`, `--name`, `--author`, `--body`,
/// `--kind`, `--project`, `--session`)
/// — the CLI renames nothing and parses nothing: raw values travel to the core,
/// which validates them and owns the error text. `doctor` is a setup/diagnostic verb
/// outside the `todo`/`note` domains (ADR 0014) and takes no flags.
enum Command {
    case help
    case doctor
    case todoList(filter: String?, text: String?, status: String?, assignee: String?, lane: String?)
    case todoAdd(title: String, status: String?, assignee: String?, lane: String?, origin: String?)
    case todoSet(
        id: String,
        title: String?,
        status: String?,
        assignee: String?,
        description: String?
    )
    case todoMove(id: String, lane: String?, index: String?)
    case todoComplete(id: String)
    case todoArchive
    case todoCommentAdd(id: String, author: String?, body: String)
    case todoCommentList(id: String)
    case laneAdd(name: String)
    case laneRename(from: String, to: String)
    case laneDelete(name: String)
    case noteRename(from: String, to: String)
    case noteCapture(kind: String, project: String?, session: String?)
    case noteStats(project: String?)
    case noteLinks
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
    case "lane":
        return try parseLane(Array(args.dropFirst()))
    case "note":
        return try parseNote(Array(args.dropFirst()))
    default:
        throw UsageError()
    }
}

// MARK: - todo

/// `todo list|add|set|move|complete|archive|comment` — the todo verbs (the agent
/// surface). An absent flag is `nil` (the core's default); a flag with no value is
/// `""`, which the core rejects or reads as the empty field. That is the one rule
/// every verb below follows, so no verb repeats a validation the core already owns.
private func parseTodo(_ args: [String]) throws -> Command {
    guard let verb = args.first else { throw UsageError() }
    // `comment` takes a sub-verb of its own (`add` / `list`), so it parses
    // before the shared `--flag value` pass.
    if verb == "comment" {
        return try parseTodoComment(Array(args.dropFirst()))
    }
    let flags = try parseFlags(Array(args.dropFirst()))
    switch verb {
    case "list":
        try accept(flags, ["--filter", "--text", "--status", "--assignee", "--lane"])
        return .todoList(
            filter: flags["--filter"],
            text: flags["--text"],
            status: flags["--status"],
            assignee: flags["--assignee"],
            lane: flags["--lane"])
    case "add":
        try accept(flags, ["--title", "--status", "--assignee", "--lane", "--origin"])
        // A missing `--title` is not a usage error: it is the empty title the
        // domain rejects (`emptyTitle`), exactly as the MCP server read an
        // absent argument. An absent `--origin` is `nil` (the core's `human`
        // default), not the empty string: there is no "no filer" value to reject.
        return .todoAdd(
            title: flags["--title"] ?? "",
            status: flags["--status"],
            assignee: flags["--assignee"],
            lane: flags["--lane"],
            origin: flags["--origin"])
    case "set":
        try accept(flags, ["--id", "--title", "--status", "--assignee", "--description"])
        // Every field is optional: the ones not named are left alone, in one write.
        // `--description -` is read from stdin (multi-line markdown is a quoting trap);
        // `main` recognizes the dash, because stdin belongs to the adapter.
        return .todoSet(
            id: flags["--id"] ?? "",
            title: flags["--title"],
            status: flags["--status"],
            assignee: flags["--assignee"],
            description: flags["--description"])
    case "move":
        try accept(flags, ["--id", "--lane", "--index"])
        return .todoMove(
            id: flags["--id"] ?? "",
            lane: flags["--lane"],
            index: flags["--index"])
    case "complete":
        try accept(flags, ["--id"])
        // A missing `--id` is likewise the empty id whose `unknownID` error
        // carries the open list.
        return .todoComplete(id: flags["--id"] ?? "")
    case "archive":
        // Bulk, whole-list: finished work leaves the view, nothing is deleted.
        try accept(flags, [])
        return .todoArchive
    default:
        throw UsageError()
    }
}

/// `todo comment add|list` — the ticket's discussion log (ADR 0017, amendment
/// 2026-10-20). `add`'s `--body -` is read from stdin, the precedent `todo set
/// --description -` set; `main` recognizes the dash, because stdin belongs to
/// the adapter. A missing `--id` / `--body` is the empty value the core
/// rejects, not a usage error; `--author` absent is the unnamed author.
private func parseTodoComment(_ args: [String]) throws -> Command {
    guard let verb = args.first else { throw UsageError() }
    let flags = try parseFlags(Array(args.dropFirst()))
    switch verb {
    case "add":
        try accept(flags, ["--id", "--author", "--body"])
        return .todoCommentAdd(
            id: flags["--id"] ?? "",
            author: flags["--author"],
            body: flags["--body"] ?? "")
    case "list":
        try accept(flags, ["--id"])
        return .todoCommentList(id: flags["--id"] ?? "")
    default:
        throw UsageError()
    }
}

// MARK: - lane

/// `lane add|rename|delete` — the lane lifecycle, the one part of the lane surface the
/// app used to own alone. The names travel raw: the core trims and owns every rejection
/// (empty name, duplicate, unknown lane), exactly as it does for the app's tree.
private func parseLane(_ args: [String]) throws -> Command {
    guard let verb = args.first else { throw UsageError() }
    let flags = try parseFlags(Array(args.dropFirst()))
    switch verb {
    case "add":
        try accept(flags, ["--name"])
        return .laneAdd(name: flags["--name"] ?? "")
    case "rename":
        try accept(flags, ["--from", "--to"])
        return .laneRename(from: flags["--from"] ?? "", to: flags["--to"] ?? "")
    case "delete":
        try accept(flags, ["--name"])
        return .laneDelete(name: flags["--name"] ?? "")
    default:
        throw UsageError()
    }
}

// MARK: - note

/// `note rename|capture|stats|links` — the four note verbs. `rename` is the rename whose
/// wikilink rewriting a raw `mv` cannot reproduce (ADR 0009); `capture` is the
/// raw memory-note write whose naming/queue invariant a plain file write cannot
/// own (ADR 0016); `stats` is the read-only memory-health readout; `links` is the
/// read-only link-graph health readout (broken links and orphan notes).
private func parseNote(_ args: [String]) throws -> Command {
    guard let verb = args.first else { throw UsageError() }
    let flags = try parseFlags(Array(args.dropFirst()))
    switch verb {
    case "rename":
        try accept(flags, ["--from", "--to"])
        return .noteRename(from: flags["--from"] ?? "", to: flags["--to"] ?? "")
    case "capture":
        try accept(flags, ["--kind", "--project", "--session"])
        // An unknown `--kind` is a domain failure, not a usage one: `main`
        // maps it to a `CaptureKind` and the core rejects it. `--project` /
        // `--session` are absent → nil (use the default); present but empty is
        // the `""` the core rejects, exactly as a missing value elsewhere.
        return .noteCapture(
            kind: flags["--kind"] ?? "",
            project: flags["--project"],
            session: flags["--session"])
    case "stats":
        try accept(flags, ["--project"])
        // Absent `--project` reports every project; a present value narrows to
        // that folder name (no match is an empty list, not an error).
        return .noteStats(project: flags["--project"])
    case "links":
        // The vault-health readout: the whole bikeshed, no filters.
        try accept(flags, [])
        return .noteLinks
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
