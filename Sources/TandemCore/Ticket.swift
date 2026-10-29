import Foundation

/// A single ticket: the only entity in the domain (GLOSSARY).
///
/// A ticket carries the full shape — where it is in the workflow (`status`), whether it
/// is set aside (`archived`), what it is about (`description`), who is on it (`assignee`),
/// who filed it (`origin`), and which cycle lane it belongs to (`lane`) — plus what it
/// always had: a title, a created time, and a completion time (ADR 0017, ADR 0020).
///
/// **`done` is derived, not a field.** In memory it is `status == .done`. The store file
/// no longer carries the legacy boolean — the migrator retired it once the build that
/// reads a `status`-only file was installed. Reading a file that still has `done` honours
/// it: only an older writer can make `done` and `status` disagree, and its click is a real
/// instruction, so `done` wins on read and the next write reconciles `status` (and drops
/// the key). The CLI's JSON still writes `done`, because that is a wire contract older
/// than the ticket model; only the store omits it.
///
/// `link` is an optional bikeshed-root-relative path to a note (e.g. `projects/App.md`).
/// Like `lane`, it is **additive** to schema v1 — absent on a ticket that predates it, and
/// the envelope version does not move ([ADR 0006]).
///
/// The custom `Codable` conformance pins the on-disk and on-the-wire date format to
/// ISO-8601 UTC (e.g. `2026-10-09T14:03:22Z`) regardless of the encoder/decoder's
/// configured date strategy — so the store file and the CLI's JSON always agree.
public struct Ticket: Identifiable, Codable, Hashable, Sendable {
    public let id: UUID
    public var title: String
    /// Where the ticket is in the workflow. Transitions are free-form: the value is
    /// validated, never the move.
    public var status: Status
    /// Set aside: out of the default views, still in the store. Any ticket can be
    /// archived; archiving a done ticket is how a list is cleared without deleting.
    public var archived: Bool
    /// The body: markdown, written by the session that shaped the ticket. `nil` when the
    /// title says everything. Trimmed at write time; an empty result means `nil`.
    public var description: String?
    /// The human (`human`), or an agent's name. `nil` when nobody is on it.
    public var assignee: String?
    /// Who filed the ticket: `human`, or the name of the agent that filed it. Written
    /// once, at add, and never changed — `let`, so the type itself enforces the rule.
    /// A ticket that predates the field has no key in the file and reads as `human`:
    /// the human's own list was filed by the human (ADR 0020).
    public let origin: String
    /// The cycle lane, or `nil` for the default (unnamed) area, which always renders
    /// first. Written as `lane` on disk; a file still under the v1 key `section` is read
    /// fine and copied aside before the write that retires it.
    public var lane: String?
    /// The ticket's number within its lane, one-based — the `42` of `TAN-42`
    /// (ticket 6598E091). `nil` for a ticket in the default area, which carries no
    /// number and no ref. Issued by the store, never reused: the lane holds the counter
    /// (`Lane.lastNumber`), so a moved-away number is not handed out twice.
    public var number: Int?
    /// The ticket's ref as the store renders it — `KEY-N`, the lane's key plus
    /// `number`. **Derived, never stored:** the store fills it on every read from the
    /// lane's key, so a lane rename that keeps the key cannot drift it. `nil` when the
    /// ticket has no number.
    public var ref: String?
    /// An optional link to a Bikeshed note: a bikeshed-root-relative path like
    /// `projects/App.md`, or `nil` for an unlinked ticket. Set in the GUI only ([ADR 0010]).
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
        origin: String = "human",
        lane: String? = nil,
        number: Int? = nil,
        ref: String? = nil,
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
        self.origin = origin
        self.lane = lane
        self.number = number
        self.ref = ref
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

    /// The wire keys. `Ticket`'s JSON shape is what the CLI emits and what the legacy
    /// import reads: `lane` is spelled **`lane`**, and the v1 key `section` is read
    /// beside it and kept forever.
    ///
    /// **The reader takes both names; the encoder emits the renamed one.** The `lane`
    /// key is decoded *beside* the v1 `section` one, and the renamed name wins when a
    /// file carries both — a hand edit is not an error. The encoder emits `lane` alone.
    /// The reader tolerates both forever — a `.bak`, a restored file or a hand edit can
    /// be v1-named forever — the same shape as the `done` mirror's decoding half. The
    /// flip was gated on the both-reading build being installed, and the file writer it
    /// guarded is gone: `tickets.json` is read once and never written (ADR 0031).
    private enum CodingKeys: String, CodingKey {
        case id, title, done, status, archived, description, assignee, origin, link, createdAt, completedAt
        /// The lane number and its rendered ref (ticket 6598E091). The number is the
        /// stored half; `ref` is derived by the store on read and written on the wire
        /// only, so it always agrees with the lane's current key.
        case number, ref
        /// The v1 key for the lane — read beside `lane` and kept forever.
        case lane = "section"
        /// The renamed key, what the writer emits.
        case renamedLane = "lane"
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        // Required fields: a missing or mistyped value throws, which the Store
        // treats as an invalid file (backup + restart) — never silently discarded.
        self.id = try c.decode(UUID.self, forKey: .id)
        self.title = try c.decode(String.self, forKey: .title)

        // The mirror decides when a legacy file carries it; a file written after the
        // migrator has `status` alone. A ticket written before `status` existed has no
        // such key and reads as `backlog`; so does a value this build does not recognise
        // — it cannot know what a later build meant, and flattening one ticket's stage is
        // recoverable where calling the whole file corrupt is not.
        let legacyDone = try c.decodeIfPresent(Bool.self, forKey: .done)
        let storedStatus = try c.decodeIfPresent(String.self, forKey: .status)
        let status = storedStatus.map(Status.stored(raw:)) ?? .backlog
        if let legacyDone {
            self.status = legacyDone ? .done : (status == .done ? .backlog : status)
        } else {
            self.status = status
        }

        // Additive to schema v1: absent reads as the default.
        self.archived = try c.decodeIfPresent(Bool.self, forKey: .archived) ?? false
        self.description = try c.decodeIfPresent(String.self, forKey: .description)
        self.assignee = try c.decodeIfPresent(String.self, forKey: .assignee)
        // Additive to schema v1: a file written before the field existed has no key —
        // as does a key explicitly written `null` — and both read as `human`, the
        // filer every pre-existing ticket actually has (ADR 0020).
        self.origin = try c.decodeIfPresent(String.self, forKey: .origin) ?? "human"
        // The lane is read under either name — the renamed one wins when a file carries
        // both. The writer emits the renamed `lane` key.
        let laneFromRenamedKey = try c.decodeIfPresent(String.self, forKey: .renamedLane)
        let laneFromV1Key = try c.decodeIfPresent(String.self, forKey: .lane)
        self.lane = laneFromRenamedKey ?? laneFromV1Key
        // Additive: a store written before the ref scheme has no numbers, and a ticket
        // in the default area never has one. `ref` is read so a wire round-trip keeps
        // the field; the store overwrites it from the lane's key on every read.
        self.number = try c.decodeIfPresent(Int.self, forKey: .number)
        self.ref = try c.decodeIfPresent(String.self, forKey: .ref)
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
        // The mirror, derived. The store omits it (the migrator retired the key); the CLI
        // result encoding uses a plain encoder and keeps it for the documented wire shape.
        if encoder.userInfo[.omitLegacyDoneMirror] == nil {
            try c.encode(done, forKey: .done)
        }
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
        // Unlike the other additive fields, origin is never `null`: a ticket's filer
        // is always known, so the first write after the field exists stamps it on
        // every ticket (ADR 0020).
        try c.encode(origin, forKey: .origin)
        if let lane {
            try c.encode(lane, forKey: .renamedLane)
        } else {
            try c.encodeNil(forKey: .renamedLane)
        }
        // The number and the ref ride the wire explicitly, `null` included, like every
        // additive field: the shape never varies by ticket. A ticket in the default
        // area has neither.
        if let number {
            try c.encode(number, forKey: .number)
        } else {
            try c.encodeNil(forKey: .number)
        }
        if let ref {
            try c.encode(ref, forKey: .ref)
        } else {
            try c.encodeNil(forKey: .ref)
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

extension CodingUserInfoKey {
    /// Set on the store's encoder only: its file no longer carries the legacy `done`
    /// mirror (the migrator). The CLI's result encoding uses a plain encoder, so the
    /// wire shape keeps the derived boolean.
    static let omitLegacyDoneMirror = CodingUserInfoKey(rawValue: "tandem.omitLegacyDoneMirror")!
}
