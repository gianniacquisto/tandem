import Foundation
import SQLite3

/// A thin wrapper over the system **libsqlite3** C API — the storage engine under the
/// store's single `tandem.db` (ADR 0031). Hand-rolled rather than a package: the API
/// ships on every platform the core could target, and the migrations the store needs
/// are about fifty lines (ticket 6B96343A, "Decided against").
///
/// The wrapper owns mechanics only: open/close, statements, transactions, the
/// `busy_timeout` that turns contention into the store's "locked" error at the seam,
/// and the backup API that rolls `tandem.db.bak` (ADR 0018). It carries no schema and
/// no domain vocabulary.
///
/// **A connection per operation.** A `Store` is `Sendable` and may be used from several
/// threads at once, so there is no shared mutable handle: every read opens its own
/// (read-only) connection and every write its own write connection, with WAL making the
/// two see one consistent database. SQLite serialises writers; the timeout is what the
/// caller's ~2 s budget rides on.
final class SQLiteDatabase {
    enum Mode {
        /// Open an existing database read-write; create it when absent.
        case readWrite
        /// Open an existing database without creating it, and without waiting for a
        /// writer (the `doctor` lock probe).
        case readWriteNoWait
        /// Open an existing database read-only. Reads never write (ADR 0001).
        case readOnly
    }

    private(set) var handle: OpaquePointer
    private var isClosed = false

    /// The contention window the store's protocol promised (ADR 0001): a writer waits
    /// ~2 s for another process's transaction, then the op fails as "locked".
    static let busyTimeoutMilliseconds: Int32 = 2_000

    init(url: URL, mode: Mode = .readWrite) throws {
        var connection: OpaquePointer?
        var flags: Int32 = SQLITE_OPEN_FULLMUTEX
        var timeout = Self.busyTimeoutMilliseconds
        switch mode {
        case .readWrite:
            flags |= SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE
        case .readWriteNoWait:
            flags |= SQLITE_OPEN_READWRITE
            timeout = 0
        case .readOnly:
            flags |= SQLITE_OPEN_READONLY
        }
        let rc = sqlite3_open_v2(url.path, &connection, flags, nil)
        guard rc == SQLITE_OK, let connection else {
            let message = connection.map { String(cString: sqlite3_errmsg($0)) }
                ?? String(cString: sqlite3_errstr(rc))
            if let connection { sqlite3_close(connection) }
            throw SQLiteError(code: rc, message: "could not open \(url.path): \(message)")
        }
        self.handle = connection
        sqlite3_busy_timeout(connection, timeout)
        if mode != .readOnly {
            // WAL: readers see a consistent snapshot without taking a lock (ADR 0001),
            // and writers serialise on one file. The mode is a property of the database
            // file, not of the connection, so read it and set it only when it is not
            // already WAL: re-asserting it on every open is a write-side operation that
            // answers SQLITE_BUSY the moment another connection holds the write lock,
            // and the busy handler does not cover it — that is the lost update the
            // concurrency suite caught.
            // A fresh file is in rollback mode and the open that created it can set WAL
            // without contention. Two connections racing to create the same home are the
            // one case that waits: the loser re-checks the mode, and a winner's switch is
            // as good as its own.
            if try scalarString("PRAGMA journal_mode")?.lowercased() != "wal" {
                try switchToWAL()
            }
        }
    }

    deinit { close() }

    /// Turn WAL on, waiting out a connection that is creating the same file at the same
    /// moment. `PRAGMA journal_mode=WAL` is not a read: it takes the write lock, and
    /// SQLite answers `SQLITE_BUSY` immediately there rather than consulting the busy
    /// handler. Success is the mode reading `wal` — from this attempt or from a peer's.
    private func switchToWAL() throws {
        var lastError: Error?
        for attempt in 0..<40 {
            do {
                _ = try scalarString("PRAGMA journal_mode=WAL")
                return
            } catch let error as SQLiteError
                where error.code == SQLITE_BUSY || error.code == SQLITE_LOCKED {
                lastError = error
                if try scalarString("PRAGMA journal_mode")?.lowercased() == "wal" { return }
                usleep(useconds_t(25_000 * (attempt + 1)))
            }
        }
        if let lastError { throw lastError }
    }

    /// Close once; later calls are no-ops, so a deterministic `close()` beside the
    /// deinit's own cannot double-close the handle.
    func close() {
        guard !isClosed else { return }
        isClosed = true
        sqlite3_close(handle)
    }

    // MARK: - Statements

    /// Run a statement that returns no rows (DDL, PRAGMA, INSERT/UPDATE/DELETE).
    func execute(_ sql: String) throws {
        try run(sql)
    }

    func run(_ sql: String, _ values: [SQLiteValue] = []) throws {
        let statement = try prepare(sql)
        defer { sqlite3_finalize(statement) }
        try bind(values, to: statement)
        let rc = sqlite3_step(statement)
        guard rc == SQLITE_DONE || rc == SQLITE_ROW else {
            throw error(rc, sql: sql)
        }
    }

    /// Run a query and map each row. The row is only valid inside `map`.
    func rows<T>(
        _ sql: String,
        _ values: [SQLiteValue] = [],
        _ map: (SQLiteRow) throws -> T
    ) throws -> [T] {
        let statement = try prepare(sql)
        defer { sqlite3_finalize(statement) }
        try bind(values, to: statement)
        var result: [T] = []
        while true {
            let rc = sqlite3_step(statement)
            switch rc {
            case SQLITE_ROW:
                result.append(try map(SQLiteRow(statement: statement)))
            case SQLITE_DONE:
                return result
            default:
                throw error(rc, sql: sql)
            }
        }
    }

    /// The first column of the first row as an integer, or `nil` for no rows.
    func scalarInt(_ sql: String, _ values: [SQLiteValue] = []) throws -> Int? {
        try rows(sql, values) { $0.int(0) }.first
    }

    /// The first column of the first row as text, or `nil` for no rows or SQL null.
    func scalarString(_ sql: String, _ values: [SQLiteValue] = []) throws -> String? {
        try rows(sql, values) { $0.textOrNil(0) }.first ?? nil
    }

    // MARK: - Transactions

    /// One transaction: the atomic write the format retires the temp+fsync+rename dance
    /// for. `IMMEDIATE` takes the write lock up front, so a read-modify-write cannot
    /// interleave with another writer — the lost-update class is SQLite's, not ours.
    /// A throw rolls everything back; nothing half-applies.
    func transaction<T>(_ body: () throws -> T) throws -> T {
        try run("BEGIN IMMEDIATE")
        do {
            let value = try body()
            try run("COMMIT")
            return value
        } catch {
            try? run("ROLLBACK")
            throw error
        }
    }

    // MARK: - Backup (ADR 0018)

    /// Copy this database to `url` through SQLite's own backup API — one complete
    /// generation, never a half-copied file. Writes to a same-directory temp and renames
    /// it over the destination, so a failed copy leaves the previous backup untouched.
    func backUp(to url: URL) throws {
        let fm = FileManager.default
        let parent = url.deletingLastPathComponent()
        let temp = parent.appendingPathComponent(
            ".\(url.lastPathComponent)-\(UUID().uuidString).tmp")
        try fm.createDirectory(at: parent, withIntermediateDirectories: true)
        var destination: OpaquePointer?
        let openRC = sqlite3_open(temp.path, &destination)
        guard openRC == SQLITE_OK, let destination else {
            if let destination { sqlite3_close(destination) }
            try? fm.removeItem(at: temp)
            throw SQLiteError(code: openRC,
                              message: "could not open the backup at \(temp.path): "
                                     + String(cString: sqlite3_errstr(openRC)))
        }
        guard let backup = sqlite3_backup_init(destination, "main", handle, "main") else {
            let rc = sqlite3_errcode(destination)
            let message = String(cString: sqlite3_errmsg(destination))
            sqlite3_close(destination)
            try? fm.removeItem(at: temp)
            throw SQLiteError(code: rc, message: "could not start the backup: \(message)")
        }
        let rc = sqlite3_backup_step(backup, -1)
        let finishRC = sqlite3_backup_finish(backup)
        if rc == SQLITE_DONE, finishRC == SQLITE_OK {
            // The copy is a byte copy of a WAL-mode database; put it back in rollback
            // journal mode so the one-file backup opens read-only without a `-shm`
            // beside it. Restoring it and opening it again turns WAL back on.
            sqlite3_exec(destination, "PRAGMA journal_mode=DELETE", nil, nil, nil)
        }
        // Close the destination before moving it: sqlite's own handles must be gone, and
        // the bytes must be on disk, or the moved file can read as an empty database.
        sqlite3_close(destination)
        guard rc == SQLITE_DONE, finishRC == SQLITE_OK else {
            try? fm.removeItem(at: temp)
            let code = finishRC != SQLITE_OK ? finishRC : rc
            throw SQLiteError(code: code,
                              message: "could not copy the database to \(url.path): "
                                     + String(cString: sqlite3_errstr(code)))
        }
        do {
            if fm.fileExists(atPath: url.path) {
                try fm.removeItem(at: url)
            }
            try fm.moveItem(at: temp, to: url)
        } catch {
            try? fm.removeItem(at: temp)
            throw SQLiteError(code: SQLITE_IOERR,
                              message: "could not roll the backup into place at \(url.path): \(error)")
        }
    }

    // MARK: - Private

    private func prepare(_ sql: String) throws -> OpaquePointer {
        var statement: OpaquePointer?
        let rc = sqlite3_prepare_v2(handle, sql, -1, &statement, nil)
        guard rc == SQLITE_OK, let statement else {
            throw error(rc, sql: sql)
        }
        return statement
    }

    private func bind(_ values: [SQLiteValue], to statement: OpaquePointer) throws {
        for (index, value) in values.enumerated() {
            let position = Int32(index + 1)
            switch value {
            case .null:
                sqlite3_bind_null(statement, position)
            case .text(let text):
                // SQLITE_TRANSIENT: SQLite copies the bytes, so the Swift string's
                // lifetime is ours to end.
                sqlite3_bind_text(statement, position, text, -1, Self.transient)
            case .int(let number):
                sqlite3_bind_int64(statement, position, Int64(number))
            case .real(let number):
                sqlite3_bind_double(statement, position, number)
            }
        }
    }

    private func error(_ code: Int32, sql: String) -> SQLiteError {
        let message = String(cString: sqlite3_errmsg(handle))
        return SQLiteError(code: code, message: "\(message) (\(sql))")
    }

    /// `SQLITE_TRANSIENT` is a C macro (a function-pointer cast), not a symbol Swift can
    /// import; this is the same value.
    private static let transient = unsafeBitCast(
        -1, to: sqlite3_destructor_type.self)
}

/// One bound value. Optional columns bind `.null` explicitly — a bound NULL, never a
/// mistyped string.
enum SQLiteValue {
    case null
    case text(String)
    case int(Int)
    case real(Double)

    static func textOrNull(_ value: String?) -> SQLiteValue {
        value.map(SQLiteValue.text) ?? .null
    }

    static func intOrNull(_ value: Int?) -> SQLiteValue {
        value.map(SQLiteValue.int) ?? .null
    }
}

/// A row from a stepped statement. Valid only inside the `rows` closure — the statement
/// is reset when it returns.
struct SQLiteRow {
    let statement: OpaquePointer

    func textOrNil(_ index: Int32) -> String? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL,
              let bytes = sqlite3_column_text(statement, index)
        else { return nil }
        return String(cString: bytes)
    }

    func text(_ index: Int32) -> String { textOrNil(index) ?? "" }

    func intOrNil(_ index: Int32) -> Int? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL else { return nil }
        return Int(sqlite3_column_int64(statement, index))
    }

    func int(_ index: Int32) -> Int { intOrNil(index) ?? 0 }

    func realOrNil(_ index: Int32) -> Double? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL else { return nil }
        return sqlite3_column_double(statement, index)
    }
}

/// The wrapper's own failure: the raw sqlite result code plus the engine's message.
/// The store translates it — `SQLITE_BUSY`/`SQLITE_LOCKED` become the domain's
/// "locked", everything else an I/O failure with the store's words (ADR 0007).
struct SQLiteError: Error, Sendable {
    let code: Int32
    let message: String
}
