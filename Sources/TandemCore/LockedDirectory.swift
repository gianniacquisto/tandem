import Foundation
import Darwin

/// The file-storage mechanics shared by Tandem's two sidecar-locked domains —
/// the todo `Store` (ADR 0001) and the notes `Bikeshed` (ADR 0004): an
/// exclusive `flock` on a sidecar lockfile with bounded retry, atomic
/// same-directory temp-file + fsync + `rename(2)` writes, and orphan temp
/// cleanup under the lock.
///
/// This is the *mechanism* only. It owns no schema, no recovery policy, and no
/// error vocabulary: each domain keeps its own file shape, its own business
/// rules, and its own errors, and translates `LockedDirectoryError` back into them
/// at the seam. So neither domain depends on the other, and changing a retry
/// window, the fsync call, or the orphan age is one edit here rather than two.
struct LockedDirectory: Sendable {
    /// The sidecar lockfile (created if missing, never renamed — ADR 0001; it
    /// lives outside the bikeshed directory, per ADR 0004).
    let lockURL: URL
    /// Same-directory temp files are named `<tempPrefix><uuid>.tmp`.
    let tempPrefix: String
    /// The tree the orphan sweep walks.
    let cleanRoot: URL
    /// Whether the orphan sweep recurses — true for the bikeshed, whose temps sit
    /// beside each note; false for the store, one flat file per directory.
    let cleanRecursively: Bool

    // Contention + cleanup tuning (ADR 0001, reused by ADR 0004).
    static let lockTimeout: TimeInterval = 2.0
    static let backoffRangeMicros: ClosedRange<UInt32> = 10_000...50_000   // 10–50 ms, in µs
    static let orphanMaxAge: TimeInterval = 60        // seconds
    static let tempSuffix = ".tmp"

    /// Acquire the exclusive `flock` and run `body` under it, orphan temps
    /// cleaned first. The lock is released when `body` returns or throws.
    /// `flock` releases on process death, so there is no stale-lock recovery to
    /// build. Throws `LockedDirectoryError.locked` after the bounded retry window.
    func withLock<T: Sendable>(_ body: () throws -> T) throws -> T {
        let fd = try acquireLock()
        defer { releaseLock(fd) }
        cleanOrphans()
        return try body()
    }

    /// A non-destructive probe: is the sidecar lock currently held by another
    /// process? Tries a non-blocking exclusive `flock` and, when it wins,
    /// releases it immediately — so a free lock stays free and nothing is
    /// written. A missing lockfile is a free lock (no writer has run yet) and
    /// the probe never creates one. Used by `tandem-cli doctor` (ADR 0014).
    func isLocked() -> Bool {
        let fd = open(lockURL.path, O_RDWR)   // no O_CREAT: a missing lockfile is free
        guard fd >= 0 else { return false }
        defer { close(fd) }
        if flock(fd, LOCK_EX | LOCK_NB) == 0 {
            _ = flock(fd, LOCK_UN)
            return false
        }
        return errno == EWOULDBLOCK
    }

    /// Write `data` to `url` atomically: same-directory temp file, fsync,
    /// `rename(2)`. On any failure the temp is removed and the error rethrown.
    /// Call only while holding the lock.
    func atomicWrite(_ data: Data, to url: URL) throws {
        let fm = FileManager.default
        let parent = url.deletingLastPathComponent()
        try fm.createDirectory(at: parent, withIntermediateDirectories: true)
        let tmp = parent.appendingPathComponent(
            "\(tempPrefix)\(UUID().uuidString)\(Self.tempSuffix)")
        do {
            try data.write(to: tmp, options: [])
            try Self.fsync(tmp)
            if Darwin.rename(tmp.path, url.path) != 0 {
                throw LockedDirectoryError.posix(op: "rename", code: errno)
            }
        } catch {
            try? fm.removeItem(at: tmp)
            throw error
        }
    }

    // MARK: - Locking (private)

    /// Acquire the exclusive `flock` on the sidecar lockfile with bounded retry.
    /// Returns an open fd that must be `close`d (closing releases the lock).
    private func acquireLock() throws -> Int32 {
        try FileManager.default.createDirectory(at: cleanRoot, withIntermediateDirectories: true)
        let fd = open(lockURL.path, O_RDWR | O_CREAT, 0o644)
        guard fd >= 0 else {
            throw LockedDirectoryError.posix(op: "open lockfile", code: errno)
        }
        let deadline = Date().addingTimeInterval(Self.lockTimeout)
        while true {
            if flock(fd, LOCK_EX | LOCK_NB) == 0 { return fd }
            if Date() >= deadline {
                close(fd)
                throw LockedDirectoryError.locked
            }
            usleep(UInt32.random(in: Self.backoffRangeMicros))
        }
    }

    private func releaseLock(_ fd: Int32) {
        if fd >= 0 { close(fd) }
    }

    // MARK: - Atomicity (private)

    private static func fsync(_ url: URL) throws {
        let fd = open(url.path, O_RDONLY)
        guard fd >= 0 else { return }
        defer { close(fd) }
        guard Darwin.fsync(fd) == 0 else {
            throw LockedDirectoryError.posix(op: "fsync", code: errno)
        }
    }

    /// Remove orphaned `<tempPrefix>*<tempSuffix>` files older than 60 s
    /// (crash leftovers). Called after acquiring the lock; the age threshold
    /// guarantees in-flight writes are never touched.
    private func cleanOrphans() {
        let fm = FileManager.default
        let cutoff = Date().addingTimeInterval(-Self.orphanMaxAge)
        if cleanRecursively {
            guard let walker = fm.enumerator(
                at: cleanRoot,
                includingPropertiesForKeys: [.contentModificationDateKey]
            ) else { return }
            for case let url as URL in walker where isTemp(url.lastPathComponent) {
                guard
                    let values = try? url.resourceValues(forKeys: [.contentModificationDateKey]),
                    let mod = values.contentModificationDate,
                    mod < cutoff
                else { continue }
                try? fm.removeItem(at: url)
            }
        } else {
            guard let entries = try? fm.contentsOfDirectory(atPath: cleanRoot.path) else { return }
            for name in entries where isTemp(name) {
                let url = cleanRoot.appendingPathComponent(name)
                guard
                    let attrs = try? fm.attributesOfItem(atPath: url.path),
                    let mod = attrs[.modificationDate] as? Date,
                    mod < cutoff
                else { continue }
                try? fm.removeItem(at: url)
            }
        }
    }

    private func isTemp(_ name: String) -> Bool {
        name.hasPrefix(tempPrefix) && name.hasSuffix(Self.tempSuffix)
    }
}

/// The storage mechanism's own failures. Each domain translates these into its
/// own error vocabulary (`DomainError` / `StoreIOError` for the store,
/// `BikeshedError` / `BikeshedIOError` for the bikeshed), so user-facing messages
/// stay domain-specific.
enum LockedDirectoryError: Error, Sendable, Equatable {
    /// The lock could not be acquired within the bounded retry window (~2 s).
    case locked
    /// A POSIX call failed (`op` names it, `code` is `errno`).
    case posix(op: String, code: Int32)
}
