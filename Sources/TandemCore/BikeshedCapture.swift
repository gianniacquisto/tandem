import Foundation

/// The five kinds of raw memory note `capture` writes. Each maps 1:1 to the
/// curated section the consolidation pass folds it into
/// (decision→Decisions, gotcha→Gotchas, fact→Facts, work→Where left off,
/// thread→Threads), so the kind also labels the filename for the consolidator
/// (raw-layer design decision 2). A `String` raw value so the CLI can carry the
/// `--kind` token without a second table of its own.
public enum CaptureKind: String, Sendable, CaseIterable {
    case decision
    case gotcha
    case fact
    case work
    case thread
}

extension Bikeshed {

    /// `capture(project:kind:body:session:at:)`: append one raw memory note to
    /// the hidden per-project capture layer (raw-layer design decision 9). The
    /// note lands at `projects/<project>/.raw/<session>/<utc>-<kind>-<slug>.md`
    /// — a hidden subtree, so it is out of the curated wikilink graph by
    /// construction, not by a rule agents must remember: `list`, `notePaths`,
    /// `search`, and `resolve` all prune it (decision 5). The filename is the
    /// invariant a plain filesystem write cannot own — the UTC stamp orders the
    /// queue, the kind labels it, and a same-second/same-slug collision gets a
    /// `-2`, `-3`, … suffix rather than overwriting a note (decision 3).
    ///
    /// `project` and `session` are each one visible folder name: no empty, `.`,
    /// or `..` component, no leading dot, and no `/`, `#`, or `|`. The `.raw`
    /// component is the core's own, so the hidden subtree needs no exemption
    /// from the naming rules. The body is written verbatim — the core never
    /// parses note text. Returns the bikeshed-root-relative path written.
    public func capture(
        project: String,
        kind: CaptureKind,
        body: String,
        session: String,
        at date: Date = Date()
    ) throws -> OperationResult<String> {
        try Self.validateCaptureComponent(project)
        try Self.validateCaptureComponent(session)
        let folder = "projects/\(project)/.raw/\(session)"
        let base = "\(Self.captureUTC(from: date))-\(kind.rawValue)-\(Self.captureSlug(from: body, fallback: kind.rawValue))"
        let path = try locked {
            var candidate = base
            var suffix = 2
            while let url = fileURL(for: "\(folder)/\(candidate).md"),
                  FileManager.default.fileExists(atPath: url.path)
            {
                candidate = "\(base)-\(suffix)"
                suffix += 1
            }
            let relative = "\(folder)/\(candidate).md"
            guard let url = fileURL(for: relative) else {
                throw BikeshedError.invalidCaptureName(name: relative)   // unreachable: components validated above
            }
            try atomicWrite(Data(body.utf8), to: url)
            return relative
        }
        return OperationResult(path)
    }

    // MARK: - Naming (private)

    /// A `capture` component is one visible folder name: no empty, `.`, or `..`
    /// (a bikeshed escape), no leading dot (hidden — the `.raw` folder is the
    /// core's own, never a caller's), and no `/`, `#`, or `|` (the path and
    /// wikilink separators). Throws `invalidCaptureName` naming the bad value.
    private static func validateCaptureComponent(_ value: String) throws {
        guard !value.isEmpty,
              value != ".", value != "..",
              !value.hasPrefix("."),
              !value.contains("/"),
              !value.contains("#"),
              !value.contains("|")
        else {
            throw BikeshedError.invalidCaptureName(name: value)
        }
    }

    /// The `<utc>` filename stamp, UTC `yyyyMMdd'T'HHmmss'Z'`
    /// (e.g. `20261018T140211Z`; raw-layer design decision 4). Built per call:
    /// capture is a one-shot write, so a shared mutable formatter would buy
    /// nothing and cost `Sendable`.
    private static func captureUTC(from date: Date) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "UTC")
        formatter.dateFormat = "yyyyMMdd'T'HHmmss'Z'"
        return formatter.string(from: date)
    }

    /// The `<slug>` for a capture's filename: the body's first non-empty line,
    /// with a leading `#`/whitespace run stripped, lowercased, every run of
    /// non-`[a-z0-9]` collapsed to one `-`, trimmed of `-`, and truncated to 40
    /// characters. An empty result falls back to the kind, so the name is
    /// always `…-<kind>-<something>.md`.
    private static func captureSlug(from body: String, fallback: String) -> String {
        let firstLine = body
            .components(separatedBy: .newlines)
            .first { !$0.trimmingCharacters(in: .whitespaces).isEmpty } ?? ""
        let slug = String(firstLine.drop { $0 == "#" || $0.isWhitespace })
            .lowercased()
            .replacingOccurrences(of: "[^a-z0-9]+", with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        guard !slug.isEmpty else { return fallback }
        return String(slug.prefix(40))
    }
}
