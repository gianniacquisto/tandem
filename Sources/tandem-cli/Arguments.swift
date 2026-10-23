import Foundation

/// The parsed command line: one of the verbs, or a request for the usage text.
/// Long flags only, and the flags mirror the core argument names exactly
/// (`--filter`, `--text`, `--status`, `--assignee`, `--lane`, `--title`, `--id`,
/// `--description`, `--index`, `--from`, `--to`, `--name`, `--author`, `--body`,
/// `--kind`, `--project`, `--session`, `--archived`, `--brief`, `--outcome`,
/// `--summary`, `--ticket`)
/// — the CLI renames nothing and parses nothing: raw values travel to the core,
/// which validates them and owns the error text. `doctor` is a setup/diagnostic verb
/// outside the `ticket`/`session`/`note` domains (ADR 0014) and takes no flags.
enum Command {
    case help
    case doctor
    case ticketList(
        filter: String?,
        text: String?,
        status: String?,
        assignee: String?,
        lane: String?,
        archived: Bool
    )
    case ticketShow(id: String)
    case ticketAdd(title: String, status: String?, assignee: String?, lane: String?, origin: String?)
    case ticketSet(
        id: String,
        title: String?,
        status: String?,
        assignee: String?,
        description: String?
    )
    case ticketMove(id: String, lane: String?, index: String?)
    case ticketComplete(id: String)
    case ticketArchive
    case ticketCommentAdd(id: String, author: String?, body: String)
    case ticketCommentList(id: String)
    case laneAdd(name: String)
    case laneRename(from: String, to: String)
    case laneDelete(name: String)
    case noteRename(from: String, to: String)
    case noteCapture(kind: String, project: String?, session: String?)
    case noteStats(project: String?)
    case noteLinks
    case sessionStart(project: String?, agent: String?, session: String?, brief: String)
    case sessionEnd(
        session: String?,
        outcome: String?,
        summary: String?,
        tokensInput: String?,
        tokensOutput: String?,
        tokensReasoning: String?,
        tokensCacheRead: String?,
        tokensCacheWrite: String?,
        cost: String?,
        model: String?)
    case sessionLink(ticket: String, session: String?)
    case sessionList(project: String?, status: String?)
    case sessionShow(session: String?)
}

/// A usage failure — no arguments, an unknown subcommand, an unknown flag, or a
/// stray token. It carries no message: the caller prints the usage text to
/// stderr and exits 2. A *missing flag value* is deliberately not one of these
/// (see `parseFlags`).
struct UsageError: Error {}

/// Parse the arguments after the program name. `--help` / `-h` as the first
/// argument is the help request; otherwise the first argument names the group
/// (`ticket` / `lane` / `note` / `session`), the second the verb, and the rest
/// are `--flag value` pairs validated against that verb.
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
    case "ticket", Usage.deprecatedAliasToken:
        // `ticket` is the verb (D1). `todo` is the deprecated alias: the same
        // parse, the same answer, and one line on stderr (see `main`).
        return try parseTicket(Array(args.dropFirst()))
    case "lane":
        return try parseLane(Array(args.dropFirst()))
    case "note":
        return try parseNote(Array(args.dropFirst()))
    case "session":
        return try parseSession(Array(args.dropFirst()))
    default:
        throw UsageError()
    }
}

// MARK: - ticket

/// `ticket list|show|add|set|move|complete|archive|comment` — the ticket verbs (the
/// agent surface). An absent flag is `nil` (the core's default); a flag with no
/// value is `""`, which the core rejects or reads as the empty field. That is
/// the one rule every verb below follows, so no verb repeats a validation the
/// core already owns.
private func parseTicket(_ args: [String]) throws -> Command {
    guard let verb = args.first else { throw UsageError() }
    // `comment` takes a sub-verb of its own (`add` / `list`), so it parses
    // before the shared `--flag value` pass.
    if verb == "comment" {
        return try parseTicketComment(Array(args.dropFirst()))
    }
    let flags = try parseFlags(Array(args.dropFirst()))
    switch verb {
    case "list":
        try accept(flags, ["--filter", "--text", "--status", "--assignee", "--lane", "--archived"])
        // `--archived` is a switch: the archive axis of the query, the one flag that
        // takes no value.
        return .ticketList(
            filter: flags["--filter"],
            text: flags["--text"],
            status: flags["--status"],
            assignee: flags["--assignee"],
            lane: flags["--lane"],
            archived: flags["--archived"] != nil)
    case "show":
        try accept(flags, ["--id"])
        // One ticket by id, whole. A missing `--id` is the empty id whose
        // `unknownID` error carries the open list, as `complete` does.
        return .ticketShow(id: flags["--id"] ?? "")
    case "add":
        try accept(flags, ["--title", "--status", "--assignee", "--lane", "--origin"])
        // A missing `--title` is not a usage error: it is the empty title the
        // domain rejects (`emptyTitle`), exactly as the MCP server read an
        // absent argument. An absent `--origin` is `nil` (the core's `human`
        // default), not the empty string: there is no "no filer" value to reject.
        return .ticketAdd(
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
        return .ticketSet(
            id: flags["--id"] ?? "",
            title: flags["--title"],
            status: flags["--status"],
            assignee: flags["--assignee"],
            description: flags["--description"])
    case "move":
        try accept(flags, ["--id", "--lane", "--index"])
        return .ticketMove(
            id: flags["--id"] ?? "",
            lane: flags["--lane"],
            index: flags["--index"])
    case "complete":
        try accept(flags, ["--id"])
        // A missing `--id` is likewise the empty id whose `unknownID` error
        // carries the open list.
        return .ticketComplete(id: flags["--id"] ?? "")
    case "archive":
        // Bulk, whole-list: finished work leaves the view, nothing is deleted.
        try accept(flags, [])
        return .ticketArchive
    default:
        throw UsageError()
    }
}

/// `ticket comment add|list` — the ticket's discussion log (ADR 0017, amendment
/// 2026-10-20). `add`'s `--body -` is read from stdin, the precedent
/// `ticket set --description -` set; `main` recognizes the dash, because stdin
/// belongs to the adapter. A missing `--id` / `--body` is the empty value the
/// core rejects, not a usage error; `--author` absent is the unnamed author.
private func parseTicketComment(_ args: [String]) throws -> Command {
    guard let verb = args.first else { throw UsageError() }
    let flags = try parseFlags(Array(args.dropFirst()))
    switch verb {
    case "add":
        try accept(flags, ["--id", "--author", "--body"])
        return .ticketCommentAdd(
            id: flags["--id"] ?? "",
            author: flags["--author"],
            body: flags["--body"] ?? "")
    case "list":
        try accept(flags, ["--id"])
        return .ticketCommentList(id: flags["--id"] ?? "")
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
        // The health readout: the whole bikeshed, no filters.
        try accept(flags, [])
        return .noteLinks
    default:
        throw UsageError()
    }
}

// MARK: - session

/// `session start|end|link|list|show` — the session record (spec `FC770B8A`): a
/// record of one agent session's run, kept as an artifact of its own.
///
/// **A session is not a ticket.** It has its own file (`sessions.json`) and its
/// own vocabulary — `running`/`ended`, `succeeded`/`failed`/`interrupted`/
/// `unknown` — so the flags here never borrow the board's `--status`.
///
/// The rule every verb below follows is the CLI's own: an absent flag takes
/// the default, a present-but-empty one is the empty value the core rejects (or
/// reads as the empty field). The defaults that need the environment or the
/// working directory — `--session`, `--agent`, `--project` — are resolved in
/// `main`, because the environment and the process belong to the adapter.
private func parseSession(_ args: [String]) throws -> Command {
    guard let verb = args.first else { throw UsageError() }
    let flags = try parseFlags(Array(args.dropFirst()))
    switch verb {
    case "start":
        try accept(flags, ["--project", "--agent", "--session", "--brief"])
        // A missing `--brief` is the empty brief the core refuses
        // (`emptySessionBrief`), exactly as a missing `--title` is the empty
        // title. `--brief -` is read from stdin (multi-line text is a quoting
        // trap); `main` recognizes the dash, because stdin belongs to the
        // adapter.
        return .sessionStart(
            project: flags["--project"],
            agent: flags["--agent"],
            session: flags["--session"],
            brief: flags["--brief"] ?? "")
    case "end":
        // The usage flags (spec `F68948C0`): absent is absent on the record —
        // today's behaviour, unchanged. `--model` is a string; the six number
        // flags are parsed in `main`, where a bad value is a domain failure
        // naming the flag.
        try accept(flags, [
            "--session", "--outcome", "--summary",
            "--tokens-input", "--tokens-output", "--tokens-reasoning",
            "--tokens-cache-read", "--tokens-cache-write",
            "--cost", "--model",
        ])
        // `--outcome` absent is `unknown` — the honest answer for a session
        // that ended without saying how; a value that is not one of the four
        // is the core's own failure. `--summary -` is stdin, as `--brief -` is.
        return .sessionEnd(
            session: flags["--session"],
            outcome: flags["--outcome"],
            summary: flags["--summary"],
            tokensInput: flags["--tokens-input"],
            tokensOutput: flags["--tokens-output"],
            tokensReasoning: flags["--tokens-reasoning"],
            tokensCacheRead: flags["--tokens-cache-read"],
            tokensCacheWrite: flags["--tokens-cache-write"],
            cost: flags["--cost"],
            model: flags["--model"])
    case "link":
        try accept(flags, ["--ticket", "--session"])
        // A missing `--ticket` is the empty id the store rejects with the
        // unknown-id failure, as `ticket complete` does.
        return .sessionLink(ticket: flags["--ticket"] ?? "", session: flags["--session"])
    case "list":
        try accept(flags, ["--project", "--status"])
        // Absent `--project` reports every project; absent `--status` every
        // status. A status that is not one of the two is a domain failure.
        return .sessionList(project: flags["--project"], status: flags["--status"])
    case "show":
        try accept(flags, ["--session"])
        return .sessionShow(session: flags["--session"])
    default:
        throw UsageError()
    }
}

// MARK: - Flags

/// The flags that take no value: a switch, there or not there. `--archived` is the
/// only one — the archive axis of `ticket list`. Every other flag takes a value, so
/// `--archived` never swallows the token after it.
private let switchFlags: Set<String> = ["--archived"]

/// Parse `tokens` as `--flag value` pairs. A flag's value is the next token
/// when one exists and `""` when the flag is last: a missing value is the
/// domain's to reject, not a usage error — the retired MCP server read an absent
/// argument the same way. A switch takes no value at all. An unknown flag or a bare
/// token where a flag is expected is a usage error; a repeated flag keeps its last
/// value.
private func parseFlags(_ tokens: [String]) throws -> [String: String] {
    var values: [String: String] = [:]
    var i = 0
    while i < tokens.count {
        let flag = tokens[i]
        guard flag.hasPrefix("--") else { throw UsageError() }
        if switchFlags.contains(flag) {
            values[flag] = ""
            i += 1
        } else if i + 1 < tokens.count {
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
