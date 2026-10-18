import Foundation

/// A single todo: the only entity in the v1 domain (GLOSSARY).
///
/// `id` is a UUIDv4, serialized in the store as a canonical 8-4-4-4-12 lowercase
/// string. `title` is trimmed at write time; there is no max length and duplicates
/// are allowed. `completedAt` is `null` while the todo is open.
///
/// `link` is an optional bikeshed-root-relative path to a Bikeshed note (e.g.
/// `projects/App.md`). Like `section`, it is **additive** to schema v1 — absent on a
/// todo that predates it, and the envelope version does not move ([ADR 0006]).
///
/// The custom `Codable` conformance pins the on-disk and on-the-wire date format to
/// ISO-8601 UTC (e.g. `2026-10-09T14:03:22Z`) regardless of the encoder/decoder's
/// configured date strategy — so the store file and the CLI's JSON always agree.
public struct Todo: Identifiable, Codable, Hashable, Sendable {
    public let id: UUID
    public var title: String
    public var done: Bool
    /// The todo's section, or `nil` for the default (ungrouped) area, which always
    /// renders first. A section is a name; the store's ordered `sections` list gives the
    /// order the groups render in. Absent in schema v1 (read as `nil`).
    public var section: String?
    /// An optional link to a Bikeshed note: a bikeshed-root-relative path like
    /// `projects/App.md`, or `nil` for an unlinked todo. Set in the GUI only ([ADR 0010]);
    /// additive to schema v1 (absent reads as `nil`).
    public var link: String?
    public let createdAt: Date
    public var completedAt: Date?

    public init(
        id: UUID = UUID(),
        title: String,
        done: Bool = false,
        section: String? = nil,
        link: String? = nil,
        createdAt: Date = .now,
        completedAt: Date? = nil
    ) {
        self.id = id
        self.title = title
        self.done = done
        self.section = section
        self.link = link
        self.createdAt = createdAt
        self.completedAt = completedAt
    }

    // MARK: - Custom Codable (ISO-8601 UTC dates, strict on required fields)

    private enum CodingKeys: String, CodingKey {
        case id, title, done, section, link, createdAt, completedAt
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        // Required fields: a missing or mistyped value throws, which the Store
        // treats as an invalid file (backup + restart) — never silently discarded.
        self.id = try c.decode(UUID.self, forKey: .id)
        self.title = try c.decode(String.self, forKey: .title)
        self.done = try c.decode(Bool.self, forKey: .done)

        // Absent in schema v1: a v1 todo is in the default (ungrouped) area.
        self.section = try c.decodeIfPresent(String.self, forKey: .section)

        // Additive to schema v1: absent (or `null`) reads as an unlinked todo.
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
        try c.encode(done, forKey: .done)
        if let section {
            try c.encode(section, forKey: .section)
        } else {
            try c.encodeNil(forKey: .section)
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
