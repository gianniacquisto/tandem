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
// semantics (`list`, `TicketQuery.parse`, `ticket(id:)` — the `show` read,
// `add`, `set`, `move`, `complete`,
// `archiveDone`, `append(ticketID:author:body:)`, `list(ticketID:)`,
// `addLane`, `renameLane`, `deleteLane`, `rename`, `capture`,
// `stats`, `health`, `DescriptionBody.required`, `Sessions.start`,
// `Sessions.end`, `Sessions.link`, `Sessions.list`, `Sessions.show`,
// `Sprints.open`, `Sprints.ticket`, `Sprints.run`, `Sprints.close`,
// `Sprints.list`, `Sprints.show`) and the
// encoding (`StoreEncoding`, `tandemErrorText`) stay in TandemCore, which the
// single test seam (`TandemCoreTests`) covers. The two things that belong to a
// process rather than to the domain live here: stdin and the environment.
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
/// `--help` prints the usage to stdout and exits 0 — at the root, the root
/// usage; below it, that group's or verb's own usage (ticket `BEE9B50F`). A
/// parse failure prints it to stderr and exits 2.
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
    case .groupHelp(let group):
        print(Usage.groups[group] ?? Usage.text)
        return 0
    case .verbHelp(let name):
        print(Usage.verbs[name] ?? Usage.text)
        return 0
    case .doctor:
        return runDoctor()
    case .ticketList(let filter, let text, let status, let assignee, let lane, let archived):
        return run {
            let query = try TicketQuery.parse(
                filter: filter,
                text: text,
                status: status,
                assignee: try resolveAssignee(assignee),
                lane: lane,
                archived: archived)
            let result = try Store().list(query)
            return try StoreEncoding.listResult(result.value, note: result.note)
        }
    case .ticketShow(let id):
        return run {
            // One ticket, whole — `list` returns summaries, so this is the read
            // that carries the description. An id the store does not carry is
            // the store's own `unknownID` failure, open list included.
            let result = try Store().ticket(id: id)
            return try StoreEncoding.ticketResult(result.value, note: result.note)
        }
    case .ticketAdd(let title, let status, let assignee, let lane, let origin):
        return run {
            let result = try Store().add(
                title,
                lane: lane,
                status: try status.map { try Status.parse(raw: $0) } ?? .backlog,
                assignee: try resolveAssignee(assignee),
                origin: try resolveOrigin(origin))
            return try StoreEncoding.ticketResult(result.value, note: result.note)
        }
    case .ticketSet(let id, let title, let status, let assignee, let description):
        return run {
            // `--description -`: the body is stdin, the precedent `note capture` set.
            // A stdin body must carry text — `descriptionBody` refuses an empty one.
            let body: Edit<String?>
            if let description {
                body = .set(try descriptionBody(description, stdin: readStandardInput))
            } else {
                body = .unchanged
            }
            let result = try Store().set(
                id,
                title: edit(title) { $0 },
                status: try edit(status) { try Status.parse(raw: $0) },
                assignee: assignee.map { .set(try resolveAssignee($0)) } ?? .unchanged,
                description: body)
            return try StoreEncoding.ticketResult(result.value, note: result.note)
        }
    case .ticketMove(let id, let lane, let index):
        return run {
            let result = try Store().move(id, toLane: lane, at: try parseIndex(index))
            return try StoreEncoding.ticketResult(result.value, note: result.note)
        }
    case .ticketComplete(let id):
        return run {
            let result = try Store().complete(id)
            return try StoreEncoding.ticketResult(result.value, note: result.note)
        }
    case .ticketArchive:
        return run {
            let result = try Store().archiveDone()
            return try StoreEncoding.archivedResult(result.value, note: result.note)
        }
    case .ticketCommentAdd(let id, let author, let body):
        return run {
            // `--body -`: the comment is stdin, the precedent `ticket set
            // --description -` set; stdin belongs to the adapter.
            let text = body == "-" ? try readStandardInput() : body
            let result = try Store().append(
                ticketID: id,
                author: author ?? "",
                body: text)
            return try StoreEncoding.commentResult(result.value, note: result.note)
        }
    case .ticketCommentList(let id):
        return run {
            let result = try Store().list(ticketID: id)
            return try StoreEncoding.commentsResult(result.value, note: result.note)
        }
    case .laneAdd(let name):
        return run {
            let result = try Store().addLane(name)
            return try StoreEncoding.lanesResult(result.value, note: result.note)
        }
    case .laneRename(let from, let to):
        return run {
            let result = try Store().renameLane(from, to: to)
            return try StoreEncoding.lanesResult(result.value, note: result.note)
        }
    case .laneDelete(let name):
        return run {
            let result = try Store().deleteLane(name)
            return try StoreEncoding.laneDeletionResult(result.value, note: result.note)
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
    case .noteLinks:
        return run {
            let result = try Bikeshed().health()
            return try StoreEncoding.bikeshedHealthResult(result.value)
        }
    case .sessionStart(let project, let agent, let session, let brief):
        return run {
            let result = try Sessions().start(
                id: try sessionID(session),
                project: project ?? captureProjectDefault(),
                agent: sessionAgent(agent),
                brief: try sessionBrief(brief))
            return try StoreEncoding.sessionResult(result.value, note: result.note)
        }
    case .sessionEnd(
        let session, let outcome, let summary,
        let tokensInput, let tokensOutput, let tokensReasoning,
        let tokensCacheRead, let tokensCacheWrite, let cost, let model):
        return run {
            // The usage flags (spec `F68948C0`): absent is absent on the
            // record, so an `end` without them is today's behaviour. When
            // usage is present, the stamp posts one `usage` comment on every
            // ticket the record links — through the core's own comment append,
            // and only when this call stamped, so the comment cannot
            // double-post (`end` stamps once).
            let tokens = try usageTokens(
                input: tokensInput, output: tokensOutput, reasoning: tokensReasoning,
                cacheRead: tokensCacheRead, cacheWrite: tokensCacheWrite)
            let usageCost = try usageCost(cost)
            let usageModel = usageModel(model)
            let reported = tokens != nil || usageCost != nil || usageModel != nil
            let result = try Sessions().end(
                id: try sessionID(session),
                outcome: try sessionOutcome(outcome),
                summary: try sessionSummary(summary),
                tokens: tokens,
                cost: usageCost,
                model: usageModel,
                onStamp: reported ? { try postUsageComments(for: $0) } : nil)
            return try StoreEncoding.sessionResult(result.value, note: result.note)
        }
    case .sessionLink(let ticket, let session):
        return run {
            // The handle first: a session that names nothing is a failure
            // before the board is even consulted.
            let id = try sessionID(session)
            // The store owns "which ticket is this id?" — the CLI asks it and
            // links what it gets back. An id the store does not carry is the
            // store's own `unknownID` failure, open list included.
            let linked = try Store().ticket(id: ticket)
            let result = try Sessions().link(id: id, ticket: linked.value.id)
            return try StoreEncoding.sessionResult(result.value, note: result.note ?? linked.note)
        }
    case .sessionList(let project, let status):
        return run {
            let result = try Sessions().list(
                project: project,
                status: try sessionStatus(status))
            return try StoreEncoding.sessionsResult(result.value, note: result.note)
        }
    case .sessionShow(let session):
        return run {
            let id = try sessionID(session)
            let result = try Sessions().show(id: id)
            // The notes a session wrote are derived from the files — the
            // capture folder is the session id — so the record and its notes
            // cannot drift apart. The record's own `tickets` are the other
            // half of what the session carried.
            let notes = try Bikeshed().sessionNotes(session: id).value
            return try StoreEncoding.sessionDetailResult(result.value, notes: notes, note: result.note)
        }
    case .sprintOpen(let lane, let mode, let session):
        return run {
            // The opening session's handle is the flag's, else the host's —
            // and an unset environment is a domain failure, the session
            // group's rule: an invented id is a record nothing can look up
            // again.
            let result = try Sprints().open(
                lane: lane,
                mode: try sprintMode(mode),
                session: try sessionID(session))
            return try StoreEncoding.sprintResult(result.value, note: result.note)
        }
    case .sprintTicket(let lane, let ticket):
        return run {
            // The handle first: the lane's open sprint is resolved before the
            // board is asked, so a mistyped lane is reported as one.
            let id = try ticketID(ticket)
            let number = try openSprintNumber(lane: lane)
            // An id the store does not carry is the core's own
            // `unknownTicketID` failure — the whole board, archive included.
            let result = try Sprints().ticket(
                lane: lane, number: number, ticketID: id, on: Store())
            return try StoreEncoding.sprintResult(result.value, note: result.note)
        }
    case .sprintRun(let lane, let note):
        return run {
            let body = try sprintNote(note)
            let number = try openSprintNumber(lane: lane)
            let result = try Sprints().run(lane: lane, number: number, note: body)
            return try StoreEncoding.sprintResult(result.value, note: result.note)
        }
    case .sprintClose(let lane, let note):
        return run {
            // `--note` optional: absent is the stamp alone; `--note -` is
            // stdin, whose empty body the core refuses.
            let body: String? = try note.map { try sprintNote($0) }
            let number = try closeSprintNumber(lane: lane)
            let result = try Sprints().close(lane: lane, number: number, note: body)
            return try StoreEncoding.sprintResult(result.value, note: result.note)
        }
    case .sprintList(let lane, let status):
        return run {
            let result = try Sprints().list(lane: lane, status: try sprintStatus(status))
            return try StoreEncoding.sprintsResult(result.value, note: result.note)
        }
    case .sprintShow(let lane, let number):
        return run {
            let result = try Sprints().show(lane: lane, number: try sprintNumber(number))
            return try StoreEncoding.sprintResult(result.value, note: result.note)
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

/// `--origin self` means the agent running this command, the same `$TANDEM_AGENT`
/// lookup as `--assignee self`. An absent flag means the human: the app, the human's
/// face, files as human, so the core's default is the human's truth — which is why the
/// skill tells an agent that files a ticket to pass `self`, not to rely on the default
/// (ADR 0020). Any other value passes straight through to the core, which owns the
/// spelling rule.
func resolveOrigin(_ raw: String?) throws -> String {
    guard raw == "self" else { return raw ?? "human" }
    guard let name = ProcessInfo.processInfo.environment["TANDEM_AGENT"],
          !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    else {
        throw DomainError.agentNameUnset
    }
    return name
}

/// The `--description` body of a `ticket set`: the flag's own value, or the text
/// piped in when the flag is `-`. A stdin body must carry text — an empty pipe,
/// a failed heredoc and an unset variable all arrive as the same empty body,
/// and writing it would erase the description silently (ADR 0024) — so it is
/// refused with `DomainError.emptyDescriptionBody`, the way `ticket comment add`
/// refuses an empty comment body. `--description ""` is the deliberate clear:
/// it travels through untouched, and the store still takes it.
func descriptionBody(_ flag: String, stdin: () throws -> String) throws -> String {
    guard flag == "-" else { return flag }
    return try DescriptionBody.required(stdin())
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

// MARK: - session

/// A session record's handle: the `--session` value when the flag is there,
/// else `$OPENCODE_SESSION_ID` — the host's id for the session it is running.
///
/// **The fallback is the environment's, never an invented one.** A capture may
/// carry a generated token because a note is filed by path; a session record is
/// found by its handle, so a made-up id is a record nothing can look up again.
/// An unset variable is a domain failure naming it, and the core's
/// `emptySessionID` stays the backstop for a present-but-empty value.
func sessionID(_ raw: String?) throws -> String {
    guard let raw else { return try sessionIDDefault() }
    return raw
}

/// `$OPENCODE_SESSION_ID`, the one place a session's handle comes from when
/// the caller does not name it.
func sessionIDDefault() throws -> String {
    guard let id = ProcessInfo.processInfo.environment["OPENCODE_SESSION_ID"],
          !id.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    else {
        throw DomainError.sessionIDUnset
    }
    return id
}

/// A session's agent: the `--agent` value when the flag is there, else
/// `$TANDEM_AGENT` — nobody (`nil`) when the environment names no agent. The
/// core trims the name and reads an empty one as no agent.
func sessionAgent(_ raw: String?) -> String? {
    guard let raw else {
        return ProcessInfo.processInfo.environment["TANDEM_AGENT"]
    }
    return raw
}

/// A session's brief: the flag's own value, or the text piped in when the flag
/// is `-`. A stdin body must carry text — an empty pipe, a failed heredoc and
/// an unset variable all arrive as the same empty body, and a brief is what
/// the record is *for* — so it is refused with the core's own
/// `emptySessionBrief` rather than filed as no brief at all.
func sessionBrief(_ raw: String) throws -> String {
    guard raw == "-" else { return raw }
    let body = try readStandardInput()
    guard !body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
        throw DomainError.emptySessionBrief
    }
    return body
}

/// A session's summary: absent is no summary, and a `--summary -` body that
/// carries no text is refused — the same empty-pipe rule `--description -`
/// follows (ADR 0024). The summary is optional in the core (an empty one is
/// simply no summary); the *stdin path* is where an empty body is a failed
/// write rather than a deliberate silence.
func sessionSummary(_ raw: String?) throws -> String? {
    guard let raw else { return nil }
    guard raw == "-" else { return raw }
    let body = try readStandardInput()
    guard !body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
        throw DomainError.emptySessionSummary
    }
    return body
}

/// How a session ended: absent is `unknown` — the honest answer for a session
/// that ended without saying how — and a value that is not one of the four is
/// the core's own failure, naming them.
func sessionOutcome(_ raw: String?) throws -> SessionOutcome {
    guard let raw else { return .unknown }
    guard let outcome = SessionOutcome(rawValue: raw) else {
        throw DomainError.invalidSessionOutcome(raw)
    }
    return outcome
}

// MARK: - sprint

/// A sprint's mode: the `--mode` value; absent is `serial` — the skill's
/// default, stored either way. A value that is not one of the two is the
/// core's own failure, naming them.
func sprintMode(_ raw: String?) throws -> SprintMode {
    guard let raw else { return .serial }
    guard let mode = SprintMode(rawValue: raw) else {
        throw DomainError.invalidSprintMode(raw)
    }
    return mode
}

/// The status a sprint list is filtered by: absent is every state, and a
/// value that is not one of the two is the core's own failure, naming them.
func sprintStatus(_ raw: String?) throws -> SprintStatus? {
    guard let raw else { return nil }
    guard let status = SprintStatus(rawValue: raw) else {
        throw DomainError.invalidSprintStatus(raw)
    }
    return status
}

/// `sprint show`'s number — the second half of the pair handle. A value that
/// is not a whole number is a domain failure naming it.
func sprintNumber(_ raw: String) throws -> Int {
    guard let number = Int(raw) else { throw DomainError.invalidSprintNumber(raw) }
    return number
}

/// A ticket id for the batch: parsed as a UUID, the type the core takes. A
/// value that does not parse is the same unknown-ticket failure a
/// well-formed id the store does not carry gets — `session link`'s rule,
/// "a malformed id is the same failure, not a crash".
func ticketID(_ raw: String) throws -> UUID {
    guard let id = UUID(uuidString: raw) else {
        throw DomainError.unknownTicketID(raw)
    }
    return id
}

/// A sprint note: the flag's own value, or the text piped in when the flag
/// is `-`. The empty body is left to the core's own refusal
/// (`emptySprintNote`) — an empty pipe is not a landing without words, and
/// the core owns the rule the way it owns every stdin body.
func sprintNote(_ raw: String) throws -> String {
    guard raw == "-" else { return raw }
    return try readStandardInput()
}

/// The number of the lane's open sprint — the record `sprint ticket` and
/// `sprint run` act on, resolved from the file because the lane-only verbs
/// name no number. A lane holds at most one open record (`open` refuses a
/// second), so the first open row is the one.
func openSprintNumber(lane: String) throws -> Int {
    let open = try Sprints().list(lane: lane, status: .open)
    guard let sprint = open.value.first else {
        throw DomainError.noOpenSprint(lane: lane)
    }
    return sprint.number
}

/// The number `sprint close` acts on: the lane's open sprint — or, when the
/// lane's sprint has already closed, the lane's newest record, whose `close`
/// answers unchanged with its note (the stamp is once, so a re-run of the
/// skill's close step cannot restate it). A lane with no records at all is
/// the no-open-sprint failure.
func closeSprintNumber(lane: String) throws -> Int {
    if let open = try Sprints().list(lane: lane, status: .open).value.first {
        return open.number
    }
    let records = try Sprints().list(lane: lane)
    guard let newest = records.value.first else {
        throw DomainError.noOpenSprint(lane: lane)
    }
    return newest.number
}

// MARK: - session usage (spec F68948C0)

/// The token counts the usage flags report: absent is absent on the record,
/// a value that is not a non-negative whole number is a domain failure naming
/// the flag.
func usageTokens(
    input: String?,
    output: String?,
    reasoning: String?,
    cacheRead: String?,
    cacheWrite: String?
) throws -> SessionTokens? {
    func count(_ flag: String, _ raw: String?) throws -> Int? {
        guard let raw else { return nil }
        guard let value = Int(raw), value >= 0 else {
            throw DomainError.invalidUsageFlag(flag: flag, value: raw)
        }
        return value
    }
    let inCount = try count("tokens-input", input)
    let outCount = try count("tokens-output", output)
    let reasoningCount = try count("tokens-reasoning", reasoning)
    let cacheReadCount = try count("tokens-cache-read", cacheRead)
    let cacheWriteCount = try count("tokens-cache-write", cacheWrite)
    if inCount == nil, outCount == nil, reasoningCount == nil,
       cacheReadCount == nil, cacheWriteCount == nil {
        return nil
    }
    return SessionTokens(
        input: inCount, output: outCount, reasoning: reasoningCount,
        cacheRead: cacheReadCount, cacheWrite: cacheWriteCount)
}

/// The cost the `--cost` flag reports, in dollars: absent is absent, a value
/// that is not a non-negative number is a domain failure naming the flag.
func usageCost(_ raw: String?) throws -> Double? {
    guard let raw else { return nil }
    guard let value = Double(raw), value >= 0, value.isFinite else {
        throw DomainError.invalidUsageFlag(flag: "cost", value: raw)
    }
    return value
}

/// The model the `--model` flag reports: absent is absent; an empty value is
/// no model, the way the core reads an empty agent name.
func usageModel(_ raw: String?) -> String? {
    guard let raw else { return nil }
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
}

/// The one `usage` comment the CLI posts on every ticket a session carried:
/// author `usage`, one plain-integer line naming the session id, the model and
/// the counts plus the cost —
/// `Usage — ses_xxx · z-ai/glm-5.3-flash`, then one line per reported piece:
/// `input 238,171`, `output 22,294`, `cache read 5,346,816`, `cache write 0`,
/// `reasoning 21,021`, `cost $0.22`. A piece the harness did not report is
/// left out entirely, never a zero dressed up as a count; a piece it reported
/// as zero stays. Plain text throughout — the comment sheet draws the body as
/// stored, so markdown decoration would read as literal characters.
///
/// The author constant is namespaced on a type, **not a top-level `let`**:
/// main.swift runs `exit(dispatch(…))` before the bottom-of-file top-level
/// lets initialize, so a global read from inside `dispatch` reads an
/// uninitialized String — a segfault, not a wrong word.
enum UsageComment {
    /// The fallback author when the record carries no agent — an unnamed
    /// author is a gap in the record, and `usage` at least says what the
    /// line is.
    static let author = "usage"

    /// The comment's author: the record's agent — the pass name the session
    /// started with — falling back to `usage` when the record carries none.
    static func author(for session: Session) -> String {
        session.agent ?? author
    }
}

func usageCommentBody(for session: Session) -> String {
    // The counts arrive unformatted (opencode stores `5346816`), and a bare
    // integer that size is hard to read — group them. en_US, not the process
    // locale: the board's readers are agents and humans across locales, and
    // the number must parse back the same everywhere.
    let formatter = NumberFormatter()
    formatter.numberStyle = .decimal
    formatter.locale = Locale(identifier: "en_US")

    var lines: [String] = []
    var header = "Usage — \(session.id)"
    if let model = session.model {
        header += " · \(model)"
    }
    lines.append(header)
    func metric(_ name: String, _ value: Int?) {
        guard let value else { return }
        let grouped = formatter.string(from: NSNumber(value: value)) ?? String(value)
        lines.append("\(name) \(grouped)")
    }
    if let tokens = session.tokens {
        metric("input", tokens.input)
        metric("output", tokens.output)
        metric("cache read", tokens.cacheRead)
        metric("cache write", tokens.cacheWrite)
        metric("reasoning", tokens.reasoning)
    }
    if let cost = session.cost {
        lines.append("cost " + String(format: "$%.2f", cost))
    }
    return lines.joined(separator: "\n")
}

/// Post the usage comment on every ticket the ended record links, through the
/// core's own comment append. Called only when this `end` stamped, so a second
/// `end` posts nothing — the record's immutability is the comment's. A ticket
/// that has vanished from the store since the link fails the call — the end
/// itself has landed, and a retry is safe (it stamps nothing, so it posts
/// nothing) — but the failure is named, never swallowed.
func postUsageComments(for session: Session) throws {
    guard !session.tickets.isEmpty else { return }
    let store = Store()
    for ticket in session.tickets {
        try store.append(
            ticketID: ticket.uuidString,
            author: UsageComment.author(for: session),
            body: usageCommentBody(for: session))
    }
}

/// The status a list is filtered by: absent is every status, and a value that
/// is not one of the two is the core's own failure, naming them.
func sessionStatus(_ raw: String?) throws -> SessionStatus? {
    guard let raw else { return nil }
    guard let status = SessionStatus(rawValue: raw) else {
        throw DomainError.invalidSessionStatus(raw)
    }
    return status
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
