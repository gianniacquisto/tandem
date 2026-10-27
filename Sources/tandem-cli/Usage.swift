/// The `--help` texts: syntax only (ADR 0009). The agent-facing descriptions live
/// in the `tandem` skill, so help and the skill are not two homes for the same
/// contract — `--help` carries the grammar, the skill carries the meaning.
///
/// `text` is the root usage. `groups` answers `<group> --help` (that group's
/// verbs), `verbs` answers `<group> <verb> --help` (that verb's line plus the
/// flag semantics the root's terse lines leave out — the stdin dash, the
/// `self` resolution, the environment defaults). Every text repeats the line it
/// answers for, so a help probe reads standalone.
enum Usage {
    static let text = """
    tandem-cli — Tandem's agent-facing command line.

    Usage:
      tandem-cli ticket list [--filter open|done|all] [--status <value[,value]>]
                              [--assignee <name|self>] [--lane <name>] [--text <string>]
                              [--archived]
      tandem-cli ticket show --id <id>
      tandem-cli ticket add --title <string> [--status <value>] [--assignee <name|self>]
                             [--lane <name>] [--origin <name|self>] [--description <string|->]
      tandem-cli ticket set --id <id> [--title <string>] [--status <value>]
                            [--assignee <name|self>] [--description <string|->]
      tandem-cli ticket move --id <id> [--lane <name>] [--index <number>]
      tandem-cli ticket complete --id <id>
      tandem-cli ticket answer --id <id> --status <value> [--assignee <name|self>]
                               [--body <string|->]
      tandem-cli ticket archive [--id <id>]
      tandem-cli ticket comment add --id <id> [--author <name>] --body <string|->
      tandem-cli ticket comment list --id <id>

      tandem-cli lane list
      tandem-cli lane add --name <name> [--abbr <key>]
      tandem-cli lane rename --from <name> --to <name> [--abbr <key>]
      tandem-cli lane delete --name <name>
      tandem-cli note rename --from <path> --to <path>
      tandem-cli note capture --kind <decision|gotcha|fact|thread> [--project <name>] [--session <id>]
      tandem-cli note stats [--project <name>]
      tandem-cli note links
      tandem-cli session start [--project <name>] [--agent <name>] [--session <id>]
                               [--brief <string|->]
      tandem-cli session end [--session <id>] [--outcome <value>] [--summary <string|->]
                             [--tokens-input <n>] [--tokens-output <n>]
                             [--tokens-reasoning <n>] [--tokens-cache-read <n>]
                             [--tokens-cache-write <n>] [--cost <dollars>]
                             [--model <name>]
      tandem-cli session link --ticket <id> [--session <id>]
      tandem-cli session list [--project <name>] [--status <value>]
      tandem-cli session show [--session <id>]
      tandem-cli sprint open --lane <name> [--mode serial|parallel] [--session <id>]
      tandem-cli sprint ticket --lane <name> --ticket <id>
      tandem-cli sprint run --lane <name> --note <string|->
      tandem-cli sprint close --lane <name> [--number <n>] [--note <string|->]
      tandem-cli sprint list [--lane <name>] [--status open|closed]
      tandem-cli sprint show --lane <name> --number <n>
      tandem-cli doctor
      tandem-cli --help | -h

    Values:
      status    backlog | refined | ready | in-progress | in-review | done
                running | ended — a session's status, on `session list`
                open | closed — a sprint's status, on `sprint list`
      mode      serial | parallel — how a sprint's batch was built
      filter    open (default) | done | all
      assignee  a name, `self` ($TANDEM_AGENT), or "" for the unassigned
      origin    a name, `self` ($TANDEM_AGENT), or human when omitted; who filed it
      author    a name; omitted is the unnamed author
      lane      a lane name, or "" for the default area
      index     0-based within the lane; omitted appends to it
      outcome   succeeded | failed | interrupted | unknown — how a session ended
      id        a ticket's UUID, or its ref — the lane key and number, TAN-42
      key       a lane's ref key: uppercase letters and digits, unique across lanes
    """

    /// `<group> --help`: the group's verbs, the root's lines for it.
    static let groups: [String: String] = [
        "ticket": """
        tandem-cli ticket — the board's ticket surface.

        Usage:
          tandem-cli ticket list [--filter open|done|all] [--status <value[,value]>]
                                  [--assignee <name|self>] [--lane <name>] [--text <string>]
                                  [--archived]
          tandem-cli ticket show --id <id>
          tandem-cli ticket add --title <string> [--status <value>] [--assignee <name|self>]
                                 [--lane <name>] [--origin <name|self>] [--description <string|->]
          tandem-cli ticket set --id <id> [--title <string>] [--status <value>]
                                [--assignee <name|self>] [--description <string|->]
          tandem-cli ticket move --id <id> [--lane <name>] [--index <number>]
          tandem-cli ticket complete --id <id>
          tandem-cli ticket answer --id <id> --status <value> [--assignee <name|self>]
                                   [--body <string|->]
          tandem-cli ticket archive
          tandem-cli ticket comment add --id <id> [--author <name>] --body <string|->
          tandem-cli ticket comment list --id <id>

        Values:
          status    backlog | refined | ready | in-progress | in-review | done
          filter    open (default) | done | all
          assignee  a name, `self` ($TANDEM_AGENT), or "" for the unassigned
          origin    a name, `self` ($TANDEM_AGENT), or human when omitted; who filed it
          author    a name; omitted is the unnamed author
          lane      a lane name, or "" for the default area
          index     0-based within the lane; omitted appends to it
        """,
        "lane": """
        tandem-cli lane — the lane lifecycle.

        Usage:
          tandem-cli lane list
          tandem-cli lane add --name <name> [--abbr <key>]
          tandem-cli lane rename --from <name> --to <name> [--abbr <key>]
          tandem-cli lane delete --name <name>

        Values:
          key       a lane's ref key: uppercase letters and digits, unique across lanes;
                    omitted, the first three letters of the name (Tandem → TAN)
        """,
        "note": """
        tandem-cli note — the bikeshed's agent face.

        Usage:
          tandem-cli note rename --from <path> --to <path>
          tandem-cli note capture --kind <decision|gotcha|fact|thread> [--project <name>] [--session <id>]
          tandem-cli note stats [--project <name>]
          tandem-cli note links
        """,
        "session": """
        tandem-cli session — the session record.

        Usage:
          tandem-cli session start [--project <name>] [--agent <name>] [--session <id>]
                                    [--brief <string|->]
          tandem-cli session end [--session <id>] [--outcome <value>] [--summary <string|->]
                                 [--tokens-input <n>] [--tokens-output <n>]
                                 [--tokens-reasoning <n>] [--tokens-cache-read <n>]
                                 [--tokens-cache-write <n>] [--cost <dollars>]
                                 [--model <name>]
          tandem-cli session link --ticket <id> [--session <id>]
          tandem-cli session list [--project <name>] [--status <value>]
          tandem-cli session show [--session <id>]

        Values:
          status    running | ended — a session's status, on `session list`
        """,
        "sprint": """
        tandem-cli sprint — the sprint record.

        Usage:
          tandem-cli sprint open --lane <name> [--mode serial|parallel] [--session <id>]
          tandem-cli sprint ticket --lane <name> --ticket <id>
          tandem-cli sprint run --lane <name> --note <string|->
          tandem-cli sprint close --lane <name> [--number <n>] [--note <string|->]
          tandem-cli sprint list [--lane <name>] [--status open|closed]
          tandem-cli sprint show --lane <name> --number <n>

        Values:
          mode      serial | parallel — how a sprint's batch was built
          status    open | closed — a sprint's status, on `sprint list`
        """,
    ]

    /// `<group> <verb> --help`: the verb's line, plus the flag semantics the
    /// root's terse lines leave out. Keyed by the full verb name (`doctor` too).
    static let verbs: [String: String] = [
        "doctor": """
        tandem-cli doctor — the shared setup's read-only diagnostic.

        Usage:
          tandem-cli doctor
        """,
        "ticket list": """
        tandem-cli ticket list — the filtered ticket list, summaries.

        Usage:
          tandem-cli ticket list [--filter open|done|all] [--status <value[,value]>]
                                  [--assignee <name|self>] [--lane <name>] [--text <string>]
                                  [--archived]

        Values:
          status    backlog | refined | ready | in-progress | in-review | done
          filter    open (default) | done | all
          assignee  a name, `self` ($TANDEM_AGENT), or "" for the unassigned

        `--archived` is a switch and takes no value: alone it is the archive as its
        own view; named with a filter, the archive joins that filter.
        """,
        "ticket show": """
        tandem-cli ticket show — one ticket, whole.

        Usage:
          tandem-cli ticket show --id <id>

        `ticket list` prints summaries (`descriptionLength` in place of the body);
        this is the read that carries the description.

        `--id` takes the ticket's ref (`TAN-42`) as well as its UUID — the same handle
        every id-taking verb accepts.
        """,
        "ticket add": """
        tandem-cli ticket add — a new ticket.

        Usage:
          tandem-cli ticket add --title <string> [--status <value>] [--assignee <name|self>]
                                 [--lane <name>] [--origin <name|self>] [--description <string|->]

        Values:
          status    backlog | refined | ready | in-progress | in-review | done
          assignee  a name, `self` ($TANDEM_AGENT), or "" for the unassigned
          origin    a name, `self` ($TANDEM_AGENT), or human when omitted; who filed it

        `--description -` reads the body from stdin, the way `ticket set` does, so
        the body lands in the same write as the ticket. Naming the flag means the
        body must carry text: a new ticket has no earlier body to clear, so `""`
        and an empty pipe are both refused. Omit the flag for a ticket with none.
        """,
        "ticket set": """
        tandem-cli ticket set — edit one ticket's fields in one write.

        Usage:
          tandem-cli ticket set --id <id> [--title <string>] [--status <value>]
                                [--assignee <name|self>] [--description <string|->]

        Values:
          status    backlog | refined | ready | in-progress | in-review | done
          assignee  a name, `self` ($TANDEM_AGENT), or "" for the unassigned

        A flag not named leaves its field alone. `--description -` reads the body
        from stdin; `--description ""` clears the description deliberately.
        """,
        "ticket move": """
        tandem-cli ticket move — a ticket's lane and position.

        Usage:
          tandem-cli ticket move --id <id> [--lane <name>] [--index <number>]

        Values:
          lane      a lane name, or "" for the default area
          index     0-based within the lane; omitted appends to it
        """,
        "ticket complete": """
        tandem-cli ticket complete — a ticket is done.

        Usage:
          tandem-cli ticket complete --id <id>
        """,
        "ticket answer": """
        tandem-cli ticket answer — the human's answer, recorded and the hold closed.

        Usage:
          tandem-cli ticket answer --id <id> --status <value> [--assignee <name|self>]
                                   [--body <string|->]

        Values:
          status    backlog | refined | ready | in-progress | in-review | done
          assignee  a name, `self` ($TANDEM_AGENT), or "" to leave it unowned

        One write: a `## Answer — <date>` block appended to the description, then the
        status and the assignee set — the question's exit from the inbox. An absent
        `--assignee` clears the owner. `--body -` reads the body from stdin; an empty
        body is refused and the ticket is left alone.
        """,
        "ticket archive": """
        tandem-cli ticket archive — the whole done list, bulk.

        Usage:
          tandem-cli ticket archive
        """,
        "ticket comment": """
        tandem-cli ticket comment — a ticket's discussion log.

        Usage:
          tandem-cli ticket comment add --id <id> [--author <name>] --body <string|->
          tandem-cli ticket comment list --id <id>

        Values:
          author    a name; omitted is the unnamed author
        """,
        "ticket comment add": """
        tandem-cli ticket comment add — one comment on the log.

        Usage:
          tandem-cli ticket comment add --id <id> [--author <name>] --body <string|->

        Values:
          author    a name; omitted is the unnamed author

        `--body -` reads the comment from stdin.
        """,
        "ticket comment list": """
        tandem-cli ticket comment list — the log, in append order.

        Usage:
          tandem-cli ticket comment list --id <id>
        """,
        "lane list": """
        tandem-cli lane list — the store's declared lanes, empty ones included.

        Usage:
          tandem-cli lane list
        """,
        "lane add": """
        tandem-cli lane add — a new lane.

        Usage:
          tandem-cli lane add --name <name> [--abbr <key>]

        Values:
          key       a lane's ref key: uppercase letters and digits, unique across lanes;
                    omitted, the first three letters of the name (Tandem → TAN)

        The key is the `TAN` of a ticket ref (`TAN-42`): it is frozen once a ticket in
        the lane holds a number.
        """,
        "lane rename": """
        tandem-cli lane rename — a lane's name, and its key.

        Usage:
          tandem-cli lane rename --from <name> --to <name> [--abbr <key>]

        The key survives a rename; `--abbr` moves it, and a lane with a numbered ticket
        refuses — the refs a human already wrote would move with it.
        """,
        "lane delete": """
        tandem-cli lane delete — a lane, its members to the default area.

        Usage:
          tandem-cli lane delete --name <name>
        """,
        "note rename": """
        tandem-cli note rename — a note's path, wikilinks rewritten.

        Usage:
          tandem-cli note rename --from <path> --to <path>
        """,
        "note capture": """
        tandem-cli note capture — a pending capture, queued for consolidation.

        Usage:
          tandem-cli note capture --kind <decision|gotcha|fact|thread>
                                  [--project <name>] [--session <id>]

        An absent `--project` is the working directory's last path component; an
        absent `--session` is $OPENCODE_SESSION_ID.

        The verb refuses junk at the door (exit 2, nothing filed): an empty or
        whitespace-only body, and a body whose first non-empty line gives the
        filename no slug. An exact re-file — same project, kind, and body — is
        a no-op: exit 0, naming the capture already pending. A consumed capture
        (one the consolidation pass moved to `.done/`) may be filed again.
        """,
        "note stats": """
        tandem-cli note stats — the memory-health readout.

        Usage:
          tandem-cli note stats [--project <name>]

        An absent `--project` reports every project.
        """,
        "note links": """
        tandem-cli note links — the link-graph health readout.

        Usage:
          tandem-cli note links
        """,
        "session start": """
        tandem-cli session start — a session record, opened.

        Usage:
          tandem-cli session start [--project <name>] [--agent <name>] [--session <id>]
                                    [--brief <string|->]

        An absent `--project` is the working directory's last path component, an
        absent `--agent` is $TANDEM_AGENT, an absent `--session` is
        $OPENCODE_SESSION_ID. `--brief` takes the literal brief text, or
        `--brief -` which reads it from stdin — never a file path, which is
        refused.
        """,
        "session end": """
        tandem-cli session end — a session record, stamped.

        Usage:
          tandem-cli session end [--session <id>] [--outcome <value>]
                                 [--summary <string|->]
                                 [--tokens-input <n>] [--tokens-output <n>]
                                 [--tokens-reasoning <n>] [--tokens-cache-read <n>]
                                 [--tokens-cache-write <n>] [--cost <dollars>]
                                 [--model <name>]

        Values:
          outcome   succeeded | failed | interrupted | unknown — how a session ended

        A flag not named is absent on the record. `--summary -` reads from stdin;
        an absent `--outcome` is `unknown`.
        """,
        "session link": """
        tandem-cli session link — the record carries a ticket.

        Usage:
          tandem-cli session link --ticket <id> [--session <id>]
        """,
        "session list": """
        tandem-cli session list — the session records.

        Usage:
          tandem-cli session list [--project <name>] [--status <value>]

        Values:
          status    running | ended
        """,
        "session show": """
        tandem-cli session show — one session record.

        Usage:
          tandem-cli session show [--session <id>]

        An absent `--session` is $OPENCODE_SESSION_ID.
        """,
        "sprint open": """
        tandem-cli sprint open — a sprint record, opened.

        Usage:
          tandem-cli sprint open --lane <name> [--mode serial|parallel] [--session <id>]

        Values:
          mode      serial | parallel — how a sprint's batch was built

        An absent `--session` is $OPENCODE_SESSION_ID.
        """,
        "sprint ticket": """
        tandem-cli sprint ticket — a ticket joins the open sprint's batch.

        Usage:
          tandem-cli sprint ticket --lane <name> --ticket <id>
        """,
        "sprint run": """
        tandem-cli sprint run — the open sprint's run log.

        Usage:
          tandem-cli sprint run --lane <name> --note <string|->

        `--note -` reads the note from stdin.
        """,
        "sprint close": """
        tandem-cli sprint close — the lane's open sprint, stamped.

        Usage:
          tandem-cli sprint close --lane <name> [--number <n>] [--note <string|->]

        `--note -` reads the note from stdin; a close that names no note is the
        stamp alone.
        """,
        "sprint list": """
        tandem-cli sprint list — the sprint records.

        Usage:
          tandem-cli sprint list [--lane <name>] [--status open|closed]

        Values:
          status    open | closed
        """,
        "sprint show": """
        tandem-cli sprint show — one sprint record.

        Usage:
          tandem-cli sprint show --lane <name> --number <n>
        """,
    ]
}
