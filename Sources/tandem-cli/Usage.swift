/// The `--help` text: syntax only (ADR 0009). The agent-facing descriptions live
/// in the `tandem` skill, so help and the skill are not two homes for the same
/// contract — `--help` carries the grammar, the skill carries the meaning.
enum Usage {
    static let text = """
    tandem-cli — Tandem's agent-facing command line.

    Usage:
      tandem-cli ticket list [--filter open|done|all] [--status <value[,value]>]
                              [--assignee <name|self>] [--lane <name>] [--text <string>]
                              [--archived]
      tandem-cli ticket show --id <id>
      tandem-cli ticket add --title <string> [--status <value>] [--assignee <name|self>]
                             [--lane <name>] [--origin <name|self>]
      tandem-cli ticket set --id <id> [--title <string>] [--status <value>]
                            [--assignee <name|self>] [--description <string|->]
      tandem-cli ticket move --id <id> [--lane <name>] [--index <number>]
      tandem-cli ticket complete --id <id>
      tandem-cli ticket archive
      tandem-cli ticket comment add --id <id> [--author <name>] --body <string|->
      tandem-cli ticket comment list --id <id>

      tandem-cli lane add --name <name>
      tandem-cli lane rename --from <name> --to <name>
      tandem-cli lane delete --name <name>
      tandem-cli note rename --from <path> --to <path>
      tandem-cli note capture --kind <decision|gotcha|fact|work|thread> [--project <name>] [--session <id>]
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
      tandem-cli doctor
      tandem-cli --help | -h

    Values:
      status    backlog | refined | ready | in-progress | in-review | done
                running | ended — a session's status, on `session list`
      filter    open (default) | done | all
      assignee  a name, `self` ($TANDEM_AGENT), or "" for the unassigned
      origin    a name, `self` ($TANDEM_AGENT), or human when omitted; who filed it
      author    a name; omitted is the unnamed author
      lane      a lane name, or "" for the default area
      index     0-based within the lane; omitted appends to it
      outcome   succeeded | failed | interrupted | unknown — how a session ended
    """
}
