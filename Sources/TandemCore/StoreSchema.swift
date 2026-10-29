import Foundation

/// The store database's schema and the ticket rows — `<home>/tandem.db` (ADR 0031).
///
/// One database for every Tandem fact (ticket 6B96343A): the tickets and lanes the
/// ticket domain owns now, and the tables the sidecar domains (comments, sessions,
/// sprints, settings) migrate into next. `schema_migrations` is the version seam — an
/// integer version and the UTC timestamp it was applied at, one row per migration, one
/// transaction each. `store_imports` is the import seam: one row per domain whose
/// legacy file has been imported, written in the same transaction as the imported rows
/// so "imported" and "has rows" can never disagree.
///
/// **Columns are typed, never JSON strings.** The ask's "counts" is queryability
/// (tokens, cost, seconds as numbers), so only the small ordered lists that are never
/// filtered — a sprint's batch and runs, a settings roster — stay JSON text.
///
/// `tickets.position` is the stored order (ADR 0006): the array's index, never derived
/// from `createdAt`, and the array stays grouped by lane (default first, then each
/// declared lane) so the list and the board walk one shape.
enum StoreSchema {
    /// One migration: its version, a name for the error text, and its DDL.
    struct Migration {
        let version: Int
        let name: String
        let apply: @Sendable (SQLiteDatabase) throws -> Void
    }

    /// The version this build writes and reads.
    static let currentVersion = 4

    /// Every migration, in order. Append only — a shipped version's DDL never changes.
    static let migrations: [Migration] = [
        Migration(version: 1, name: "tickets and lanes") { db in
            try db.execute("""
                CREATE TABLE IF NOT EXISTS tickets (
                    id          TEXT PRIMARY KEY,
                    title       TEXT NOT NULL,
                    description TEXT,
                    status      TEXT NOT NULL,
                    archived    INTEGER NOT NULL,
                    lane        TEXT,
                    assignee    TEXT,
                    origin      TEXT NOT NULL,
                    link        TEXT,
                    position    INTEGER NOT NULL,
                    createdAt   TEXT NOT NULL,
                    completedAt TEXT
                )
                """)
            try db.execute("""
                CREATE TABLE IF NOT EXISTS lanes (
                    name     TEXT PRIMARY KEY,
                    position INTEGER NOT NULL
                )
                """)
            try db.execute("CREATE INDEX IF NOT EXISTS tickets_position ON tickets (position)")
            try db.execute("""
                CREATE TABLE IF NOT EXISTS comments (
                    ticketId TEXT NOT NULL,
                    author   TEXT NOT NULL,
                    at       TEXT NOT NULL,
                    body     TEXT NOT NULL
                )
                """)
            try db.execute("CREATE INDEX IF NOT EXISTS comments_ticket ON comments (ticketId)")
            try db.execute("""
                CREATE TABLE IF NOT EXISTS sessions (
                    id             TEXT PRIMARY KEY,
                    project        TEXT NOT NULL,
                    agent          TEXT,
                    brief          TEXT NOT NULL,
                    status         TEXT NOT NULL,
                    outcome        TEXT,
                    summary        TEXT,
                    startedAt      TEXT NOT NULL,
                    endedAt        TEXT,
                    tokensInput    INTEGER,
                    tokensOutput   INTEGER,
                    tokensReasoning INTEGER,
                    tokensCacheRead INTEGER,
                    seconds        REAL,
                    cost           REAL,
                    tickets        TEXT
                )
                """)
            try db.execute("""
                CREATE TABLE IF NOT EXISTS sprints (
                    lane     TEXT NOT NULL,
                    number   INTEGER NOT NULL,
                    session  TEXT,
                    mode     TEXT NOT NULL,
                    batch    TEXT,
                    runs     TEXT,
                    note     TEXT,
                    status   TEXT NOT NULL,
                    openedAt TEXT NOT NULL,
                    closedAt TEXT,
                    PRIMARY KEY (lane, number)
                )
                """)
            try db.execute("""
                CREATE TABLE IF NOT EXISTS settings (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                )
                """)
        },
        // The sidecar domains' record fields that v1's table sketch left out. The
        // migration is append-only: v1's DDL is what shipped on `dev`, so the new
        // columns and the sprint's `close` name land here rather than editing it.
        Migration(version: 2, name: "sidecar columns") { db in
            try db.execute("ALTER TABLE sessions ADD COLUMN tokensCacheWrite INTEGER")
            try db.execute("ALTER TABLE sessions ADD COLUMN model TEXT")
            try db.execute("ALTER TABLE sprints RENAME COLUMN note TO close")
        },
        // Ticket references (ticket 6598E091): a lane key, a per-lane ticket number and
        // the lane's issued-number counter. The backfill is the read path's `prepare()`
        // in SQL: declare every lane a ticket names, generate each lane's key
        // deterministically, and number each lane's tickets in `createdAt` order (ties
        // by UUID). The default area's tickets keep `number` NULL. One transaction, so a
        // failed backfill rolls back whole; the version row is what makes a second open a
        // no-op.
        Migration(version: 3, name: "lane keys and ticket numbers") { db in
            try db.execute("ALTER TABLE tickets ADD COLUMN number INTEGER")
            try db.execute("ALTER TABLE lanes ADD COLUMN key TEXT")
            try db.execute("ALTER TABLE lanes ADD COLUMN lastNumber INTEGER NOT NULL DEFAULT 0")

            var names = try db.rows("SELECT name FROM lanes ORDER BY position") { $0.text(0) }
            // A lane a ticket names but the lane list does not carry (a hand edit) is
            // declared too, in first-appearance order — the read path does the same, so
            // its pre-migration refs and the migration's agree.
            let dangling = try db.rows("""
                SELECT lane FROM tickets WHERE lane IS NOT NULL
                GROUP BY lane ORDER BY MIN(position)
                """) { $0.text(0) }
            var position = try db.scalarInt("SELECT COUNT(*) FROM lanes") ?? 0
            for name in dangling where !names.contains(name) {
                try db.run("INSERT INTO lanes (name, position) VALUES (?, ?)",
                           [.text(name), .int(position)])
                position += 1
                names.append(name)
            }

            var taken: Set<String> = []
            for name in names {
                let key = LaneKey.generate(from: name, taken: taken)
                taken.insert(key)
                try db.run("UPDATE lanes SET key = ? WHERE name = ?", [.text(key), .text(name)])
                let ids = try db.rows("""
                    SELECT id FROM tickets WHERE lane = ? ORDER BY createdAt, id
                    """, [.text(name)]) { $0.text(0) }
                for (index, id) in ids.enumerated() {
                    try db.run("UPDATE tickets SET number = ? WHERE id = ?",
                               [.int(index + 1), .text(id)])
                }
                try db.run("UPDATE lanes SET lastNumber = ? WHERE name = ?",
                           [.int(ids.count), .text(name)])
            }
        },
        // On hold's retired spelling (TAN-195): the domain and the wire carry `on-hold`
        // alone, so every row that still spells the old word becomes `on-hold`. One
        // UPDATE in one transaction; the version row is what makes a second open a
        // no-op. Appended rather than folded into v1, because a shipped migration's DDL
        // is history.
        Migration(version: 4, name: "on-hold replaces the retired spelling") { db in
            try db.run("UPDATE tickets SET status = ? WHERE status = ?",
                       [.text(Status.onHold.rawValue), .text(Status.retiredOnHold)])
        }
    ]

    /// The table that records what has been applied. Created outside the versioned
    /// migrations because it is the seam itself, not schema a version owns.
    static func createMigrationTable(_ db: SQLiteDatabase) throws {
        try db.execute("""
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version   INTEGER PRIMARY KEY,
                appliedAt TEXT NOT NULL
            )
            """)
    }

    /// The table that records what has been imported into the database — the durable
    /// answer to "is this database the store yet?" that the write transaction reads
    /// (ticket 6B96343A's review). Like `schema_migrations` it is the seam's own table,
    /// not schema a version owns, and it is created in the same transaction that reads
    /// it.
    static func createImportTable(_ db: SQLiteDatabase) throws {
        try db.execute("""
            CREATE TABLE IF NOT EXISTS store_imports (
                domain     TEXT PRIMARY KEY,
                source     TEXT,
                importedAt TEXT NOT NULL
            )
            """)
    }

    /// The domain whose import the ticket store's first write records.
    static let ticketsDomain = "tickets"
    /// The sidecar domains' import markers (P2). Each is written in the same
    /// transaction as its rows, so the read fallback never serves a half-import.
    static let commentsDomain = "comments"
    static let sessionsDomain = "sessions"
    static let sprintsDomain = "sprints"
    static let settingsDomain = "settings"

    /// Whether `domain` has been imported — the row is written in the same transaction
    /// as the imported rows, so seeing it means both are durable and not seeing it means
    /// neither is. A database from before the marker existed (or one caught mid-create)
    /// has no table: answer "not imported", never fail the read.
    static func isImported(_ domain: String, in db: SQLiteDatabase) throws -> Bool {
        let table = try db.scalarInt(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'store_imports'")
        guard (table ?? 0) > 0 else { return false }
        let rows = try db.scalarInt(
            "SELECT COUNT(*) FROM store_imports WHERE domain = ?", [.text(domain)])
        return (rows ?? 0) > 0
    }

    /// Record an import in the caller's transaction. `source` is the legacy file's name,
    /// or `nil` for a fresh home.
    static func markImported(
        _ domain: String, from source: String?, at date: Date, in db: SQLiteDatabase
    ) throws {
        try db.run(
            "INSERT INTO store_imports (domain, source, importedAt) VALUES (?, ?, ?)",
            [.text(domain), .textOrNull(source), .text(Timestamp.encode(date))])
    }

    /// Apply every migration newer than the recorded version, in order, one transaction
    /// each. **The version is read inside the transaction that records it** — so two
    /// connections opening a fresh home at once serialise on `BEGIN IMMEDIATE`, the
    /// first applies v1 and the second sees it and applies nothing. Reading it before
    /// the transaction let both apply v1, and the loser died on the primary key (a store
    /// I/O error, not "locked", so nothing retried).
    ///
    /// A migration that throws rolls its own transaction back and fails the open with
    /// the store's I/O error — the store is never left half-migrated.
    static func migrate(
        _ db: SQLiteDatabase,
        migrations: [Migration] = StoreSchema.migrations
    ) throws {
        // A cheap pre-check so an up-to-date database pays no write transaction per
        // write. It only decides whether there is work: the version that counts is read
        // inside each migration's transaction below.
        guard try needsMigration(db, migrations: migrations) else { return }
        for migration in migrations.sorted(by: { $0.version < $1.version }) {
            do {
                try db.transaction {
                    try createMigrationTable(db)
                    try createImportTable(db)
                    let applied = try version(of: db) ?? 0
                    guard migration.version > applied else { return }
                    try migration.apply(db)
                    try db.run(
                        "INSERT INTO schema_migrations (version, appliedAt) VALUES (?, ?)",
                        [.int(migration.version), .text(Timestamp.encode(Date()))])
                }
            } catch {
                // Name the migration that failed; the transaction already rolled back.
                if let sqlite = error as? SQLiteError {
                    throw SQLiteError(
                        code: sqlite.code,
                        message: "migration \(migration.version) (\(migration.name)) failed: "
                               + sqlite.message)
                }
                throw error
            }
        }
    }

    /// The applied version a pending migration would move past, or `nil` when there is
    /// nothing to migrate or nothing to keep a copy of. The writers take their rolling
    /// backup from this before `migrate` runs, because each migration commits in its own
    /// transaction: a copy rolled after it already holds the new version (TAN-221).
    /// A database with no seam yet (a file whose first write was killed before the seam
    /// landed) has no version and needs no pre-image: the JSON it still has is the truth.
    static func pendingMigrationVersion(of db: SQLiteDatabase) throws -> Int? {
        guard try needsMigration(db, migrations: migrations) else { return nil }
        return (try? version(of: db)) ?? nil
    }

    /// Whether the seam's tables are all present and the last migration is applied. Read
    /// without a transaction on purpose: a stale "yes" is impossible (a later version
    /// only comes from a committed migration and the version is re-read inside the
    /// transaction that applies it), and a stale "no" only costs a transaction that then
    /// finds nothing to do.
    private static func needsMigration(
        _ db: SQLiteDatabase, migrations: [Migration]
    ) throws -> Bool {
        guard let latest = migrations.map(\.version).max() else { return false }
        let tables = try db.scalarInt("""
            SELECT COUNT(*) FROM sqlite_master
            WHERE type = 'table' AND name IN ('schema_migrations', 'store_imports')
            """) ?? 0
        guard tables == 2 else { return true }
        return (try version(of: db) ?? 0) < latest
    }

    /// The highest applied migration, or `nil` when the database has no seam yet.
    static func version(of db: SQLiteDatabase) throws -> Int? {
        try db.scalarInt("SELECT MAX(version) FROM schema_migrations")
    }

    // MARK: - Ticket rows

    /// Read the ticket domain: lanes in render order, then tickets in stored order.
    ///
    /// **A version-2 database reads too** (ticket 6598E091): a read never migrates, so a
    /// home the installed build left at v2 must still list — and it answers with the refs
    /// the pending migration will make true, because `prepare()` generates the keys and
    /// numbers from the same rule the migration's backfill uses.
    static func readContents(_ db: SQLiteDatabase) throws -> StoreContents {
        let version = try version(of: db) ?? 0
        let keyed = version >= 3
        let lanes = try db.rows(keyed
            ? "SELECT name, key, lastNumber FROM lanes ORDER BY position"
            : "SELECT name, position FROM lanes ORDER BY position") { row -> Lane in
            keyed
                ? Lane(name: row.text(0), key: row.textOrNil(1) ?? "", lastNumber: row.int(2))
                : Lane(name: row.text(0), key: "", lastNumber: 0)
        }
        let todos = try db.rows(keyed ? """
            SELECT id, title, status, archived, description, assignee, origin, lane, link,
                   createdAt, completedAt, number
            FROM tickets
            ORDER BY position
            """ : """
            SELECT id, title, status, archived, description, assignee, origin, lane, link,
                   createdAt, completedAt
            FROM tickets
            ORDER BY position
            """) { row -> Ticket in
            guard let id = UUID(uuidString: row.text(0)) else {
                throw SQLiteError(code: 0, message: "a ticket row carries an invalid id \(row.text(0))")
            }
            guard let created = Timestamp.decode(row.text(9)) else {
                throw SQLiteError(code: 0,
                                  message: "a ticket row carries an unreadable createdAt \(row.text(9))")
            }
            let completed: Date?
            if let raw = row.textOrNil(10) {
                guard let date = Timestamp.decode(raw) else {
                    throw SQLiteError(code: 0,
                                      message: "a ticket row carries an unreadable completedAt \(raw)")
                }
                completed = date
            } else {
                completed = nil
            }
            return Ticket(
                id: id,
                title: row.text(1),
                // A status this build does not know reads as backlog, never a failure —
                // the same lenient rule the file's decoder kept, the retired on-hold
                // spelling included until the migration replaces it.
                status: Status.stored(raw: row.text(2)),
                archived: row.int(3) != 0,
                description: row.textOrNil(4),
                assignee: row.textOrNil(5),
                origin: row.text(6),
                lane: row.textOrNil(7),
                number: keyed ? row.intOrNil(11) : nil,
                link: row.textOrNil(8),
                createdAt: created,
                completedAt: completed)
        }
        var contents = StoreContents(lanes: lanes, todos: todos)
        // Idempotent: an up-to-date database finds nothing to declare, key or number, and
        // only the derived `ref` is refreshed; a v2 database reads as its migration will
        // leave it.
        contents.prepare()
        return contents
    }

    /// Replace the ticket domain with `contents`: one transaction's worth of rows, the
    /// positions rewritten from the array exactly as the file's order was.
    static func writeContents(_ contents: StoreContents, to db: SQLiteDatabase) throws {
        try db.run("DELETE FROM tickets")
        try db.run("DELETE FROM lanes")
        for (position, lane) in contents.lanes.enumerated() {
            try db.run("INSERT INTO lanes (name, key, lastNumber, position) VALUES (?, ?, ?, ?)",
                       [.text(lane.name), .text(lane.key), .int(lane.lastNumber), .int(position)])
        }
        for (position, ticket) in contents.todos.enumerated() {
            try db.run("""
                INSERT INTO tickets
                    (id, title, description, status, archived, lane, assignee, origin, link,
                     position, createdAt, completedAt, number)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """, [
                    .text(ticket.id.uuidString),
                    .text(ticket.title),
                    .textOrNull(ticket.description),
                    .text(ticket.status.rawValue),
                    .int(ticket.archived ? 1 : 0),
                    .textOrNull(ticket.lane),
                    .textOrNull(ticket.assignee),
                    .text(ticket.origin),
                    .textOrNull(ticket.link),
                    .int(position),
                    .text(Timestamp.encode(ticket.createdAt)),
                    ticket.completedAt.map { .text(Timestamp.encode($0)) } ?? .null,
                    .intOrNull(ticket.number)
                ])
        }
    }

    /// A lock-free count for `doctor`: open and done tickets, without loading the model.
    static func counts(_ db: SQLiteDatabase) throws -> (open: Int, done: Int) {
        let total = try db.scalarInt("SELECT COUNT(*) FROM tickets") ?? 0
        let done = try db.scalarInt("SELECT COUNT(*) FROM tickets WHERE status = ?",
                                    [.text(Status.done.rawValue)]) ?? 0
        return (open: total - done, done: done)
    }
}
