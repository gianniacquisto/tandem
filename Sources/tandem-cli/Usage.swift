/// The `--help` text: syntax only (ADR 0009). The agent-facing descriptions live
/// in the `tandem` skill, so help and the skill are not two homes for the same
/// contract — `--help` carries the grammar, the skill carries the meaning.
enum Usage {
    static let text = """
    tandem-cli — Tandem's agent-facing command line.

    Usage:
      tandem-cli todo list [--filter open|done|all] [--status <value[,value]>]
                           [--assignee <name|self>] [--lane <name>] [--text <string>]
      tandem-cli todo add --title <string> [--status <value>] [--assignee <name|self>]
                          [--lane <name>]
      tandem-cli todo set --id <id> [--title <string>] [--status <value>]
                         [--assignee <name|self>] [--description <string|->]
      tandem-cli todo move --id <id> [--lane <name>] [--index <number>]
      tandem-cli todo complete --id <id>
      tandem-cli todo archive
      tandem-cli note rename --from <path> --to <path>
      tandem-cli note capture --kind <decision|gotcha|fact|work|thread> [--project <name>] [--session <id>]
      tandem-cli note stats [--project <name>]
      tandem-cli doctor
      tandem-cli --help | -h

    Values:
      status    backlog | refined | ready | in-progress | in-review | done
      filter    open (default) | done | all
      assignee  a name, `self` ($TANDEM_AGENT), or "" for the unassigned
      lane      a lane name, or "" for the default area
      index     0-based within the lane; omitted appends to it
    """
}
