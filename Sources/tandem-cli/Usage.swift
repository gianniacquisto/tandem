/// The `--help` text: syntax only (ADR 0009). The agent-facing descriptions live
/// in the `tandem` skill, so help and the skill are not two homes for the same
/// contract — `--help` carries the grammar, the skill carries the meaning.
enum Usage {
    static let text = """
    tandem-cli — Tandem's agent-facing command line.

    Usage:
      tandem-cli todo list [--filter open|done|all]
      tandem-cli todo add --title <string>
      tandem-cli todo complete --id <string>
      tandem-cli note rename --from <path> --to <path>
      tandem-cli --help | -h
    """
}
