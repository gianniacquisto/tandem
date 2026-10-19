import Foundation

/// A single todo: the only entity in the domain (GLOSSARY).
///
/// A todo carries the ticket shape — where it is in the workflow (`status`), whether it
/// is set aside (`archived`), what it is about (`description`), who is on it (`assignee`),
/// and which cycle lane it belongs to (`lane`) — plus what it always had: a title, a
/// created time, and a completion time (ADR 0017).
///
/// **`done` is a mirror, not a field.** The legacy boolean stays on disk because a v1-era
/// reader requires it, and a missing required key is indistinguishable from corruption —
/// the failure that moved a real store aside on 2026-10-17. In memory it is derived from
/// `status == .done`, so the two cannot disagree. Reading resolves a `done`/`status`
/// disagreement in favour of `done` (only an older writer can create one, and its click is
/// a real instruction) and writing persists both, so the file self-heals.
///
/// `link` is an optional bikeshed-root-relative path to a note (e.g. `projects/App.md`).
/// Like `lane`, it is **additive** to schema v1 — absent on a todo that predates it, and
/// the envelope version does not move ([ADR 0006]).
///
/// The custom `Codable` conformance pins the on-disk and on-the-wire date format to
/// ISO-8601 UTC (e.g. `2026-10-09T14:03:22Z`) regardless of the encoder/decoder's
/// configured date strategy — so the store file and the CLI's JSON always agree.
public struct Todo: Identifiable, Codable, Hashable, Sendable {
    public let id: UUID
    public var title: String
    /// Where the ticket is in the workflow. Transitions are free-form: the value is
    /// validated, never the move.
    public var status: Status
    /// Set aside: out of the default views, still in the store. Any ticket can be
    /// archived; archiving a done ticket is how a list is cleared without deleting.
    public var archived: Bool
    /// The body: markdown, written by the session that refined the ticket. `nil` when the
    /// title says everything. Trimmed at write time; an empty result means `nil`.
    public var description: String?
    /// The human (`human`), or an agent's name. `nil` when nobody is on it.
    public var assignee: String?
    /// The cycle lane, or `nil` for the default (unnamed) area, which always renders
    /// first. Written as `section` on disk — the stored key is the migrator's business.
    public var lane: String?
    /// An optional link to a Bikeshed note: a bikeshed-root-relative path like
    /// `projects/App.md`, or `nil` for an unlinked todo. Set in the GUI only ([ADR 0010]).
    public var link: String?
    public let createdAt: Date
    public var completedAt: Date?

    /// The legacy v1 field, derived (see the type's note). It is never stored in memory,
    /// so there is no state in which it disagrees with `status`.
    public var done: Bool { status == .done }

    public init(
        id: UUID = UUID(),
        title: String,
        status: Status = .backlog,
        archived: Bool = false,
        description: String? = nil,
        assignee: String? = nil,
        lane: String? = nil,
        link: String? = nil,
        createdAt: Date = .now,
        completedAt: Date? = nil
    ) {
        self.id = id
        self.title = title
        self.status = status
        self.archived = archived
        self.description = description
        self.assignee = assignee
        self.lane = lane
        self.link = link
        self.createdAt = createdAt
        self.completedAt = completedAt
    }

    /// Move the ticket in the workflow, keeping `completedAt` truthful: entering `done`
    /// stamps it, leaving `done` clears it. Every status write goes through here so the
    /// timestamp never drifts from the state it describes.
    public mutating func setStatus(_ status: Status, at now: Date = .now) {
        if status == .done {
            if self.status != .done { completedAt = now }
        } else {
            completedAt = nil
        }
        self.status = status
    }

    // MARK: - Custom Codable (ISO-8601 UTC dates, strict on required fields)

    /// The on-disk keys. `lane` is spelled **`section`** on disk and on the wire: the
    /// stored shape is the migrator's business, not this change's (ADR 0017).
    private enum CodingKeys: String, CodingKey {
        case id, title, done, status, archived, description, assignee, link, createdAt, completedAt
        case lane = "section"
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        // Required fields: a missing or mistyped value throws, which the Store
        // treats as an invalid file (backup + restart) — never silently discarded.
        self.id = try c.decode(UUID.self, forKey: .id)
        self.title = try c.decode(String.self, forKey: .title)

        // The mirror decides. A ticket written before `status` existed has no such key
        // and reads as `backlog`; so does a value this build does not recognise — it
        // cannot know what a later build meant, and flattening one ticket's stage is
        // recoverable where calling the whole file corrupt is not.
        let legacyDone = try c.decode(Bool.self, forKey: .done)
        let storedStatus = try c.decodeIfPresent(String.self, forKey: .status)
        let status = storedStatus.flatMap(Status.parse(raw:)) ?? .backlog
        self.status = legacyDone ? .done : (status == .done ? .backlog : status)

        // Additive to schema v1: absent reads as the default.
        self.archived = try c.decodeIfPresent(Bool.self, forKey: .archived) ?? false
        self.description = try c.decodeIfPresent(String.self, forKey: .description)
        self.assignee = try c.decodeIfPresent(String.self, forKey: .assignee)
        self.lane = try c.decodeIfPresent(String.self, forKey: .lane)
        self.link = try c.decodeIfPresent(String.self, forKey: .link)

        let createdRaw = try c.decode(String.self, forKey: .createdAt)
        guard let createdAt = Timestamp.decode(createdRaw) else {
            throw CorruptStore.invalidTimestamp(field: "createdAt", value: createdRaw)
        }
        self.createdAt = createdAt

        if let completedRaw = try c.decodeIfPresent(String.self, forKey: .completedAt) {
            guard let completedAt = Timestamp.decode(completedRaw) else {
                throw CorruptStore.invalidTimestamp(field: "completedAt", value: completedRaw)
            }
            self.completedAt = completedAt
        } else {
            self.completedAt = nil
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(title, forKey: .title)
        // The mirror, derived: a v1-era reader sees exactly the `done` it understands.
        try c.encode(done, forKey: .done)
        try c.encode(status.rawValue, forKey: .status)
        try c.encode(archived, forKey: .archived)
        // Every additive field is written explicitly, `null` included, so the file shows
        // the whole shape rather than a shape that varies by ticket.
        if let description {
            try c.encode(description, forKey: .description)
        } else {
            try c.encodeNil(forKey: .description)
        }
        if let assignee {
            try c.encode(assignee, forKey: .assignee)
        } else {
            try c.encodeNil(forKey: .assignee)
        }
        if let lane {
            try c.encode(lane, forKey: .lane)
        } else {
            try c.encodeNil(forKey: .lane)
        }
        if let link {
            try c.encode(link, forKey: .link)
        } else {
            try c.encodeNil(forKey: .link)
        }
        try c.encode(Timestamp.encode(createdAt), forKey: .createdAt)
        if let completedAt {
            try c.encode(Timestamp.encode(completedAt), forKey: .completedAt)
        } else {
            try c.encodeNil(forKey: .completedAt)
        }
    }
}
