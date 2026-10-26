import Foundation

/// The four kinds of raw memory note `capture` writes. Each maps 1:1 to the
/// curated section the consolidation pass folds it into
/// (decision→Decisions, gotcha→Gotcha, fact→Facts, thread→Threads), so the
/// kind also labels the filename for the consolidator
/// (raw-layer design decision 2). A `String` raw value so the CLI can carry the
/// `--kind` token without a second table of its own.
///
/// The fifth original kind, `work`, is retired (ticket F75507ED, following
/// 0AC26CC5): the summary at `session end --summary` (ADR 0027) is the work
/// record, so the writer's set no longer carries it. Stored `*-work-*.md`
/// captures still decode, read and archive fine — the filename needs no enum
/// to be read — but `--kind work` is refused at the door.
public enum CaptureKind: String, Sendable, CaseIterable {
    case decision
    case gotcha
    case fact
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
    ///
    /// **The junk gate.** A capture is a note the consolidator folds, so the
    /// verb refuses junk at the door rather than queueing it: an empty or
    /// whitespace-only body is `emptyCaptureBody` (the ticket-description rule,
    /// mirrored), and a body whose slug is empty — the first non-empty line
    /// carries no words, so the filename would fall back to the kind's own
    /// name, the `work-work.md` junk — is `emptyCaptureSlug`. Both throw before
    /// the lock, so a refused capture creates nothing.
    ///
    /// **The duplicate no-op.** A re-file after a retry is not an error: when
    /// the project's *pending* layer already carries a capture with the same
    /// kind and byte-identical body, nothing is written and the result names
    /// the existing path with a note saying so. The archive is out of the
    /// scan — a capture the consolidation pass consumed may be filed again.
    public func capture(
        project: String,
        kind: CaptureKind,
        body: String,
        session: String,
        at date: Date = Date()
    ) throws -> OperationResult<String> {
        try Self.validateCaptureComponent(project)
        try Self.validateCaptureComponent(session)
        guard !body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw BikeshedError.emptyCaptureBody
        }
        let slug = Self.captureSlug(from: body)
        guard !slug.isEmpty else {
            throw BikeshedError.emptyCaptureSlug(kind: kind.rawValue)
        }
        let folder = "projects/\(project)/.raw/\(session)"
        let base = "\(Self.captureUTC(from: date))-\(kind.rawValue)-\(slug)"
        return try locked {
            if let existing = try existingPendingCapture(project: project, kind: kind, body: body) {
                return OperationResult(
                    existing,
                    note: "An identical capture (same project, kind, and body) is already pending at \"\(existing)\" — nothing new was filed.")
            }
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
            return OperationResult(relative)
        }
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
    /// characters. Empty when the line carries no words — the caller refuses
    /// that capture (the junk gate) rather than falling back to the kind's own
    /// name, which is how `work-work.md` junk got filed.
    private static func captureSlug(from body: String) -> String {
        let firstLine = body
            .components(separatedBy: .newlines)
            .first { !$0.trimmingCharacters(in: .whitespaces).isEmpty } ?? ""
        let slug = String(firstLine.drop { $0 == "#" || $0.isWhitespace })
            .lowercased()
            .replacingOccurrences(of: "[^a-z0-9]+", with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return String(slug.prefix(40))
    }

    // MARK: - The duplicate scan (private)

    /// The pending capture of `project` whose kind and body are exactly the
    /// ones given, bikeshed-root relative — the duplicate no-op's read, run
    /// inside the lock so the check and the write are one atomic step.
    ///
    /// The scan is the pending layer alone (`projects/<project>/.raw/**`,
    /// hidden entries pruned so the `.done/` archive is out): a capture the
    /// consolidation pass consumed may be filed again. The kind is matched from
    /// the filename — the stamp is fixed-width (`yyyyMMdd'T'HHmmss'Z'`, 16
    /// characters), so the segment after `index 16 + '-'` is the kind — and the
    /// body byte-for-byte. Returns nil when nothing pending matches.
    private func existingPendingCapture(project: String, kind: CaptureKind, body: String) throws -> String? {
        let raw = directory.appendingPathComponent("projects/\(project)/.raw")
        let wanted = "\(kind.rawValue)-"
        let payload = Data(body.utf8)
        for url in files(under: raw, recursive: true, pruningHidden: true) {
            let name = url.lastPathComponent
            guard name.lowercased().hasSuffix(".md") else { continue }
            let stem = name.dropLast(3)
            guard stem.count >= 17 else { continue }
            let afterStamp = stem.index(stem.startIndex, offsetBy: 17)
            guard stem[afterStamp...].hasPrefix(wanted) else { continue }
            let existing = (try? Data(contentsOf: url)) ?? Data()
            guard existing == payload else { continue }
            let root = directory.standardizedFileURL.path
            let path = url.standardizedFileURL.path
            guard path.hasPrefix(root + "/") else { continue }
            return String(path.dropFirst(root.count + 1))
        }
        return nil
    }
}
