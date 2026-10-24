import Foundation

/// The single source of truth for tickets (GLOSSARY: Store): one JSON file at
/// `<home>/tickets.json` or the legacy `<home>/todos.json` — the real `~/.tandem` for
/// the shipped app, `~/.tandem-dev` for anything built and run in place (`TandemHome`) —
/// read and written by the GUI and by `tandem-cli` invocations. Implements the protocol
/// from ADR 0001:
///
/// - **Locking** — writers take an exclusive `flock` on the sidecar lockfile
///   (`<home>/tandem.lock`, never renamed), held across the whole
///   read → modify → write-temp → `rename` cycle. Readers take no lock: atomic rename
///   guarantees a reader sees a complete old or new file. `flock` releases on process
///   death, so there is no stale-lock recovery to build.
/// - **Contention** — bounded retry (10–50 ms backoff, ~2 s) then a clear
///   "store locked" error.
/// - **Atomic writes** — same-directory temp `.todos-<random>.tmp`, fsync, `rename`.
///   After acquiring the lock, orphaned temps older than 60 s (crash leftovers) are
///   removed; the age threshold never touches in-flight writes.
/// - **Lenient read, strict write** — unknown extra fields are ignored; a missing or
///   mistyped required field invalidates the whole file.
/// - **Recovery** — on an invalid file, *under the lock* (so two processes can't
///   double-recover), it is renamed to `<store>.corrupt-<unix-ts>` and the store
///   restarts empty. The backup path is surfaced as a note. Artifacts are named for
///   the file they were taken from (`todos.json.corrupt-<ts>` for a corrupt
///   pre-rename file, `tickets.json.corrupt-<ts>` for a post-rename one).
/// - **Migration** — a file that still carries the legacy `done` mirror is copied aside
///   as `<store>.pre-migrator-<unix-ts>` on the first write, then rewritten with
///   `status` alone. Reads never migrate: a read-only store keeps its old shape, and the
///   decoder keeps honouring it.
/// - **Pre-rename copy** — a file that still carries the v1 names (`todos`, `sections`,
///   `section`) is copied aside as `<store>.pre-rename-<unix-ts>` on the first write
///   in the renamed shape, with the migrator's exact rules: under the lock, the bytes as
///   they were, old keys and all, colliding names suffixed, and a copy that cannot be
///   taken fails the write. The same rules cover the file rename below: the handover's
///   copy keeps the name of the file it was taken from (`todos.json.pre-rename-<ts>`,
///   whatever keys it carries), and a snapshot taken from `tickets.json` is
///   `tickets.json.pre-rename-<ts>`. Each retirement is one generation: no later write
///   copies again.
/// - **Rolling backup** — each write first copies the file it is about to replace to
///   `tickets.json.bak` (one generation, same temp + fsync + `rename` under the same lock,
///   ADR 0018), so an ordinary bad write — a kill mid-rename, a bad regex — leaves one
///   previous good copy to fall back on. On the handover the file replaced is the legacy
///   one, so `tickets.json.bak` holds `todos.json`'s bytes and the README's one-command
///   recovery is true from the first post-rename second. The first write has no `.bak`
///   (no file yet), and a failed write leaves the previous one untouched. Reads write
///   nothing.
/// - **Two names, one file** — the store resolves to **`tickets.json`** when it is
///   present (a regular file), else to the legacy **`todos.json`** when *that* is
///   present, else it is absent. *Present* means a regular file: a directory, a broken
///   link or an unreadable path at `tickets.json` is **invalid** — the corrupt path
///   fires, and the old name is never a fall-through for a broken new one. Reads read
///   both names forever (a hand-restored `.pre-rename` copy, a `.corrupt-` fixture and
///   a rolled-back store all load). **The writer writes `tickets.json` — every write.**
///   When the store it read is the legacy `todos.json`, that write is the **handover**,
///   one locked act in this order: copy `todos.json` aside as
///   `todos.json.pre-rename-<ts>` (a copy that cannot be taken fails the write), write
///   `tickets.json` with the `.bak` rolled, `fsync` the store **directory** so the
///   rename survives a crash before anything is deleted, then remove `todos.json`. A
///   remove that fails does not fail the write — the store is already written, and the
///   leftover is the fork state `doctor` names. The handover is one generation: later
///   writes never touch `todos.json` again (a human may have restored one on purpose),
///   and both files present is a fork — `tickets.json` is the store, the legacy file is
///   left exactly as it is. A fresh home (neither present) writes `tickets.json` alone.
/// - **Two names, one shape** — the decoder reads the renamed keys (`tickets`, `lanes`,
///   and a ticket's `lane`) *beside* the v1 ones (`todos`, `sections`, `section`), and
///   the renamed name wins when a file carries both. **The writer emits the renamed keys
///   alone**, and its first write over a v1-named file copies that file aside as
///   `<store>.pre-rename-<ts>` first (the bullet above). The reader keeps tolerating
///   both shapes forever — a `.bak`, a restored file or a hand edit can be v1-named
///   forever — the same shape as the `done` mirror's decoding half. The flip was gated
///   on the both-reading build being installed: a 0.15.0-era reader looks the array up
///   by name and calls a file without `todos` **corrupt**, moving it to
///   `<store>.corrupt-<ts>` and restarting empty (ADR 0006's incident).
///
/// The public surface is the ticket operations (see the `Operations` extension); the
/// locking/file mechanics are the shared `LockedDirectory` primitive (ADR 0001 / 0004),
/// which the bikeshed uses too.
public struct Store: Sendable {
    /// The store's directory (created on first write). Public so the GUI can watch it
    /// for external changes (FSEvents); the GUI never writes it directly.
    public let directory: URL
    /// The store file's new name: `<home>/tickets.json`.
    let fileURL: URL
    /// The store file's legacy name: `<home>/todos.json`.
    let legacyFileURL: URL
    /// The file the store resolves to — the resolution rule: `tickets.json` present
    /// (a regular file or not: a broken new path is invalid, never a fall-through) →
    /// that is the store; else `todos.json` present → that is the store; else
    /// `todos.json`, the path a first write would create. Public for `doctor`.
    public var storeFileURL: URL {
        Self.lstatOf(fileURL.path) == nil ? legacyFileURL : fileURL
    }
    /// The rolling backup of the file the last write replaced — the one previous
    /// good copy. Not written by reads, and not on the first write (ADR 0018).
    /// Follows the store's name, `tickets.json.bak`; on the handover it holds
    /// `todos.json`'s bytes — the state before the write that renamed the store.
    var backupURL: URL {
        URL(fileURLWithPath: fileURL.path + ".bak")
    }
    /// The sidecar lockfile (created if missing, never renamed).
    let lockURL: URL
    /// The shared storage mechanics (ADR 0001): flock + atomic write + orphan
    /// sweep, identical to the bikeshed's (ADR 0004).
    private let storage: LockedDirectory

    /// The handover's file steps, as seams: the copy-aside (step 2) and the
    /// legacy removal (step 5). The defaults are FileManager's own; tests
    /// replace them to make one step throw and pin the order's guarantees.
    /// Only the handover uses them — a post-handover write copies nothing
    /// aside and removes nothing.
    var copyAsideFile: @Sendable (URL, URL) throws -> Void = {
        try FileManager.default.copyItem(at: $0, to: $1)
    }
    /// The removal of the legacy `todos.json`, the handover's last step. A
    /// remove that fails never fails the write — the store is already written
    /// — so the closure's error is swallowed at the call site; the seam exists
    /// so a test can prove that guarantee.
    var removeLegacy: @Sendable (URL) throws -> Void = {
        try FileManager.default.removeItem(at: $0)
    }

    /// The default store: the real `~/.tandem` from the shipped app, `~/.tandem-dev`
    /// from anything built and run in place (see `TandemHome`).
    public init() {
        self.init(directory: TandemHome.directory)
    }

    /// A store rooted at an explicit directory (used by tests with a temp dir).
    public init(directory: URL) {
        self.directory = directory
        self.fileURL = directory.appendingPathComponent("tickets.json")
        self.legacyFileURL = directory.appendingPathComponent("todos.json")
        self.lockURL = directory.appendingPathComponent("tandem.lock")
        self.storage = LockedDirectory(
            lockURL: self.lockURL,
            tempPrefix: ".todos-",
            cleanRoot: directory,
            cleanRecursively: false)
    }

    /// The path's `lstat` mode, or `nil` when nothing exists there. `lstat` does not
    /// follow symlinks, so a broken link still counts as present-but-invalid.
    private static func lstatOf(_ path: String) -> mode_t? {
        var sb = stat()
        return lstat(path, &sb) == 0 ? sb.st_mode : nil
    }

    /// Take the store lock (translating the primitive's errors into the store's
    /// own vocabulary) and run `body` under it, orphan temps cleaned first.
    /// Internal (not private): the comment operations run under the store's own
    /// lock too — the sidecar append must serialize with every store writer.
    func withLock<T: Sendable>(_ body: () throws -> T) throws -> T {
        do {
            return try storage.withLock(body)
        } catch LockedDirectoryError.locked {
            throw DomainError.locked
        } catch LockedDirectoryError.posix(let op, let code) {
            throw StoreIOError.posix(op: op, code: code)
        }
    }

    // MARK: - Low-level store protocol (private)

    private enum Peek {
        case absent
        /// A readable store, plus the bytes it was decoded from (the migration check
        /// needs the raw file; the lock-free readers ignore it).
        case valid(StoreContents, raw: Data)
        case invalid
    }

    /// A lock-free read of the current file state. Distinguishes "no file yet" (a valid
    /// empty store) from "file present but corrupt" (needs recovery). Present means a
    /// regular file — a directory, a broken link or an unreadable path is invalid, never
    /// absent and never a fall-through to the other name.
    private func peek() -> Peek {
        let url = storeFileURL
        guard let mode = Self.lstatOf(url.path) else {
            return .absent
        }
        guard mode & S_IFMT == S_IFREG, let data = try? Data(contentsOf: url) else {
            return .invalid
        }
        do {
            return .valid(try Self.decode(data), raw: data)
        } catch {
            return .invalid
        }
    }

    /// Read while holding the exclusive lock. Re-checks under the lock so a process that
    /// already recovered is not double-recovered; if still corrupt, backs the file up and
    /// restarts empty. Returns the contents, when *this* call performed the backup the
    /// backup path, whether the file it read still carries the legacy `done` mirror
    /// or the v1 names — the caller writing must then copy it aside first, under the
    /// matching marker — and the file it read (the resolved store at read time): the
    /// caller writing recognizes the handover when that is the legacy `todos.json`.
    /// Internal (not private): `withLockedRead` builds on it for
    /// operations that read the store and write a sidecar.
    func readLocked() throws -> (
        contents: StoreContents,
        backupPath: String?,
        legacyMirror: Bool,
        v1Names: Bool,
        readFrom: URL
    ) {
        let url = storeFileURL
        switch peek() {
        case .absent:
            return (StoreContents(lanes: [], todos: []), nil, false, false, url)
        case .valid(let contents, let raw):
            return (contents, nil, Self.carriesLegacyMirror(raw), Self.carriesV1Names(raw), url)
        case .invalid:
            let ts = Int(Date().timeIntervalSince1970)
            let prefix = url.lastPathComponent
            var backup = directory.appendingPathComponent("\(prefix).corrupt-\(ts)")
            var suffix = 1
            let fm = FileManager.default
            while fm.fileExists(atPath: backup.path) {
                backup = directory.appendingPathComponent("\(prefix).corrupt-\(ts)-\(suffix)")
                suffix += 1
            }
            do {
                try fm.moveItem(at: url, to: backup)
            } catch {
                // Never silently discard: if we cannot back the corrupt file up, fail loudly.
                throw StoreIOError.io("could not back up corrupt store: \(error)")
            }
            return (StoreContents(lanes: [], todos: []), backup.path, false, false, url)
        }
    }

    /// Write while holding the lock: encode and hand to the shared atomic
    /// write-temp+fsync+rename primitive, taking the rolling backup of the file the
    /// write replaces just before the commit (see `backUpCurrentFile`). The write
    /// always lands in `tickets.json`, the store's name — on the handover that is
    /// the write that renames the store.
    private func writeLocked(_ contents: StoreContents) throws {
        let data = try Self.encode(contents)
        try storage.atomicWrite(data, to: fileURL) { try backUpCurrentFile() }
    }

    /// Roll the file this write is about to replace to `tickets.json.bak` — **one
    /// generation**, the state before the write. Runs under the lock, from inside the
    /// atomic write (new bytes durable, `rename` not yet done), so the backup is itself
    /// temp + fsync + `rename` and no failure mode leaves it half-written or stale-wrong:
    /// a failed write leaves the previous `.bak` untouched. The source is the file the
    /// write replaces — the resolved store, which on the handover is still the legacy
    /// `todos.json` — so `.bak` holds exactly the bytes the write is retiring, and the
    /// README's one-command recovery is true from the first post-rename second. No file
    /// yet means nothing to roll — the first write writes no backup. A backup we cannot
    /// take fails the write rather than going unrecorded (the `backUpBeforeMigrating`
    /// rule).
    private func backUpCurrentFile() throws {
        guard FileManager.default.fileExists(atPath: storeFileURL.path) else { return }
        guard let data = try? Data(contentsOf: storeFileURL) else {
            throw StoreIOError.io("could not read \(storeFileURL.path) to roll its backup")
        }
        do {
            try storage.atomicWrite(data, to: backupURL)
        } catch {
            throw StoreIOError.io("could not back up the store to \(backupURL.path): \(error)")
        }
    }

    // MARK: - Schema (private)

    /// The envelope. `lanes` and each ticket's `lane` are **additive** to schema v1,
    /// which is why the version below does not move.
    ///
    /// **The reader takes both names; the writer emits the renamed ones.** The renamed
    /// keys (`tickets`, `lanes`) are decoded *beside* the v1 ones (`todos`, `sections`),
    /// and the renamed name wins when a file carries both — a hand edit is not an error,
    /// and the next write makes the file one shape. The writer emits the renamed keys
    /// alone, and the first write over a v1-named file copies that file aside as
    /// `todos.json.pre-rename-<ts>` first. The reader keeps tolerating both shapes
    /// forever — a `.bak`, a restored file or a hand edit can be v1-named forever — the
    /// same shape as the `done` mirror's decoding half. The flip was gated on the
    /// both-reading build being installed, because a 0.15.0-era reader looks the array
    /// up by name, and a file without `todos` is **corrupt** to it: it moves the file to
    /// `todos.json.corrupt-<ts>` and restarts empty (ADR 0006's incident).
    private struct FileEnvelope: Codable {
        var version: Int
        /// The ordered lane names. Absent in a v1 file written before lanes existed.
        var lanes: [String]?
        /// The tickets. Written under the renamed key **`tickets`**; the v1 key
        /// `todos` is read beside it and kept forever.
        var todos: [Ticket]

        private enum CodingKeys: String, CodingKey {
            case version, todos
            /// The v1 lane-list key, read beside `lanes` and kept forever (ADR 0017).
            case lanes = "sections"
            /// The renamed array key, what the writer emits.
            case tickets
            /// The renamed lane-list key, read beside `sections`; what the writer emits.
            case renamedLanes = "lanes"
        }

        /// The memberwise initializer, written out because the custom `init(from:)`
        /// below suppresses the compiler's.
        init(version: Int, lanes: [String]?, todos: [Ticket]) {
            self.version = version
            self.lanes = lanes
            self.todos = todos
        }

        /// Decode the renamed names beside the v1 ones, the renamed name winning when a
        /// file carries both. A file that carries **neither** array key is invalid: this
        /// decode throws, so the store's recovery path backs the file up and restarts
        /// empty — never a silent empty list, which is the failure the recovery path
        /// exists to prevent.
        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            self.version = try c.decode(Int.self, forKey: .version)
            if let renamed = try c.decodeIfPresent([Ticket].self, forKey: .tickets) {
                self.todos = renamed
            } else {
                // Absent on both: `keyNotFound` → the file is invalid → recovery.
                self.todos = try c.decode([Ticket].self, forKey: .todos)
            }
            // The lane list is additive: absent in a file written before lanes existed,
            // and it reads as no lanes either way.
            if let renamed = try c.decodeIfPresent([String].self, forKey: .renamedLanes) {
                self.lanes = renamed
            } else {
                self.lanes = try c.decodeIfPresent([String].self, forKey: .lanes)
            }
        }

        /// The writer, spelled out: **the renamed keys** — `version`, `lanes`,
        /// `tickets`, in that legible order. It is hand-written rather than synthesized
        /// because the decoder's key set carries the v1 names too, and a synthesized
        /// encoder cannot have a `CodingKeys` case that no property matches.
        func encode(to encoder: Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            try c.encode(version, forKey: .version)
            try c.encodeIfPresent(lanes, forKey: .renamedLanes)
            try c.encode(todos, forKey: .tickets)
        }
    }

    /// **The envelope version is bumped only for a change an older reader would
    /// misread — never for additive fields.** `lanes` and `lane` are additive: an
    /// older build ignores the unknown keys and reads the file fine. Bumping to 2 made
    /// it do the opposite — an installed v1-era `tandem-mcp` read a version-2 file,
    /// called it *corrupt*, backed it up and started empty. That happened to a real
    /// store on 2026-10-17. So the writer keeps emitting version 1. Reading still
    /// accepts 2, so any file written during that window loads. The mirror's retirement
    /// is not a version bump either: a v1-era reader fails on a missing key whatever the
    /// version says, and the migrator's gate is reachability, not the file's label.
    ///
    /// **The writer emits the renamed keys** — `tickets`, `lanes`, and a ticket's
    /// `lane` — the flip this change is. It was gated on the build that reads both
    /// shapes being installed (a v1-era reader calls a `todos`-less file corrupt and
    /// restarts empty, ADR 0006's incident), and the first write over a v1-named file
    /// copies that file aside as `todos.json.pre-rename-<ts>` before it retires the old
    /// names. The decoder keeps both shapes forever.
    private static func encode(_ contents: StoreContents) throws -> Data {
        let envelope = FileEnvelope(version: 1, lanes: contents.lanes, todos: contents.todos)
        let encoder = JSONEncoder()
        // Pretty-printed: the file is intentionally human-legible (spec: Further Notes).
        encoder.outputFormatting = [.prettyPrinted]
        // The file retires the legacy `done` mirror; the CLI's JSON keeps it.
        encoder.userInfo[.omitLegacyDoneMirror] = true
        return try encoder.encode(envelope)
    }

    private static func decode(_ data: Data) throws -> StoreContents {
        // Lenient read: unknown extra fields are ignored (forward-compat); a missing or
        // mistyped required field throws (invalid file → recovery). Version 1 is the
        // current shape (with the additive lanes fields); 2 is tolerated for files
        // written during the window when the version was mistakenly bumped. The array
        // and the lane names are read under either name — the renamed one or the v1 one.
        let envelope = try JSONDecoder().decode(FileEnvelope.self, from: data)
        guard envelope.version == 1 || envelope.version == 2 else {
            throw CorruptStore.unsupportedVersion(envelope.version)
        }
        return StoreContents(lanes: envelope.lanes ?? [], todos: envelope.todos)
    }

    // MARK: - Read / mutate primitives

    /// A lock-free read (the `list` fast path). Takes the lock only to recover a corrupt
    /// file. Returns the contents and an optional recovery note. Internal (not public): the
    /// public surface is the `Operations` extension; this is the primitive it builds on.
    func readAll() throws -> (contents: StoreContents, note: String?) {
        switch peek() {
        case .absent:
            return (StoreContents(lanes: [], todos: []), nil)
        case .valid(let contents, _):
            return (contents, nil)
        case .invalid:
            let (contents, backup, _, _, _) = try withLock { try readLocked() }
            return (contents, backup.map(Self.recoveryNote))
        }
    }

    /// A read-modify-write under the exclusive lock. `body` receives the current contents
    /// (after any recovery, and normalized to the grouped-by-lane invariant) and returns
    /// `(value, changed)`; when `changed` is false no write is performed — but the lock is
    /// still taken and recovery is still checked. The contents are normalized again before
    /// the write, so every operation leaves the invariant holding. A first write over a
    /// file that still carries the legacy `done` mirror copies that file aside first (the
    /// migrator's safety net), a first write over a file that still carries the v1 names
    /// copies it aside as `todos.json.pre-rename-<ts>` — and when the store read is the
    /// legacy **file** (`todos.json`), that write is the handover: copy aside, write
    /// `tickets.json`, fsync the directory, remove the legacy file, one locked act (see
    /// the type's doc block). Every write rolls the file it replaces to
    /// `tickets.json.bak`. If `body` throws and this call performed a recovery, the
    /// failure is wrapped in a `RecoveryFailure` so the note is not lost. Internal (not
    /// public): the public surface is the `Operations` extension.
    func mutate<T: Sendable>(
        _ body: (inout StoreContents) throws -> (value: T, changed: Bool)
    ) throws -> (value: T, note: String?) {
        try withLock {
            let (initial, backup, legacyMirror, v1Names, readFrom) = try readLocked()
            let note = backup.map(Self.recoveryNote)
            var contents = initial
            contents.normalize()
            do {
                let (value, changed) = try body(&contents)
                if changed {
                    contents.normalize()
                    if legacyMirror {
                        try copyStoreAside(marker: "pre-migrator", reason: "retiring the done mirror")
                    }
                    // The handover is the write that replaces the legacy *file* — its
                    // name, whatever keys it carries — and only that: the legacy file
                    // must still be there to copy (a recovery has already moved a
                    // corrupt one aside), and a fork (both files present) reads as
                    // `tickets.json` and is never a handover.
                    let handover = readFrom == legacyFileURL
                        && FileManager.default.fileExists(atPath: legacyFileURL.path)
                    if handover {
                        try copyStoreAside(marker: "pre-rename", reason: "the store file's rename")
                    } else if v1Names {
                        try copyStoreAside(marker: "pre-rename", reason: "renaming its keys")
                    }
                    try writeLocked(contents)
                    if handover {
                        try finishHandover()
                    }
                }
                return (value, note)
            } catch {
                if let note {
                    let underlying: any TandemError = error as? any TandemError
                        ?? StoreIOError.io("\(error)")
                    throw RecoveryFailure(underlying: underlying, note: note)
                }
                throw error
            }
        }
    }

    /// The handover's epilogue, after `tickets.json` is on disk (the rename inside
    /// `atomicWrite` has happened): make the rename durable, then remove the legacy
    /// file — one locked act, the order the type's doc block pins. A directory fsync
    /// that fails leaves the legacy file in place (it is then the only durable copy;
    /// the fork state is `doctor`'s to name) but does not fail the write — the store
    /// is already written. A removal that fails likewise never fails the write: the
    /// leftover is the fork state, not a lost store.
    private func finishHandover() throws {
        guard (try? storage.fsyncDirectory()) != nil else { return }
        _ = try? removeLegacy(legacyFileURL)
    }

    /// Run `body` under the store's exclusive lock with the store read under the
    /// **same** lock (recovery and normalization included). For operations that
    /// consult the store and write elsewhere — the comment append verifies the
    /// ticket exists before appending to the sidecar, so check and write are one
    /// locked act and a ticket deleted in between cannot slip through. `body`
    /// gets the normalized contents and the recovery note when *this* call
    /// performed the backup; a throw with a note is wrapped in a
    /// `RecoveryFailure` like `mutate`, so the note is not lost. Internal (not
    /// public): the public surface is the `Operations` extension.
    /// (The closure's parameters carry no labels — function types cannot —
    /// so the body receives `(contents, note)` positionally: the normalized
    /// contents, then the recovery note, non-nil only when *this* call
    /// performed the backup.)
    func withLockedRead<T: Sendable>(
        _ body: (StoreContents, String?) throws -> T
    ) throws -> T {
        try withLock {
            let (initial, backup, _, _, _) = try self.readLocked()
            let note = backup.map(Self.recoveryNote)
            var contents = initial
            contents.normalize()
            do {
                return try body(contents, note)
            } catch {
                if let note {
                    let underlying: any TandemError = error as? any TandemError
                        ?? StoreIOError.io("\(error)")
                    throw RecoveryFailure(underlying: underlying, note: note)
                }
                throw error
            }
        }
    }

    /// Copy the store file aside, once, before the first write that retires a shape —
    /// under the migrator's marker when the file still carries the `done` mirror, under
    /// the rename's marker when it still carries the v1 names. "A list moved aside, never
    /// a list lost": the old shape stays one `mv` away for a binary that still needs it,
    /// the bytes as they were and old keys and all. The name is `<store>.<marker>-<unix-ts>`,
    /// colliding names suffixed. A copy that cannot be taken fails the write rather than
    /// being skipped — the retirement must not outpace its evidence.
    private func copyStoreAside(marker: String, reason: String) throws {
        let ts = Int(Date().timeIntervalSince1970)
        let prefix = storeFileURL.lastPathComponent
        var backup = directory.appendingPathComponent("\(prefix).\(marker)-\(ts)")
        var suffix = 1
        let fm = FileManager.default
        while fm.fileExists(atPath: backup.path) {
            backup = directory.appendingPathComponent("\(prefix).\(marker)-\(ts)-\(suffix)")
            suffix += 1
        }
        do {
            try copyAsideFile(storeFileURL, backup)
        } catch {
            throw StoreIOError.io("could not copy the store aside before \(reason): \(error)")
        }
    }

    /// Whether any ticket object in the raw file still carries the legacy `done` key.
    /// Only asked on the write path (under the lock), never on a plain read. The array
    /// is looked up under either name — the renamed one or the v1 one — so a file in the
    /// renamed shape gets the same safety copy before its `done` keys are dropped.
    private static func carriesLegacyMirror(_ data: Data) -> Bool {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return false
        }
        let tickets = (object["tickets"] as? [[String: Any]])
            ?? (object["todos"] as? [[String: Any]])
            ?? []
        return tickets.contains { $0["done"] != nil }
    }

    /// Whether the raw file still carries any v1 name — `todos` for the array, `sections`
    /// for the lane list, or `section` on a ticket for its lane. Only asked on the write
    /// path (under the lock), never on a plain read. The names are looked up beside their
    /// renamed twins — the writer now emits the renamed ones, and a file carrying a v1
    /// one, however it got there, is the one whose first renamed write takes the
    /// `pre-rename` copy.
    private static func carriesV1Names(_ data: Data) -> Bool {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return false
        }
        if object["todos"] != nil || object["sections"] != nil {
            return true
        }
        let tickets = (object["tickets"] as? [[String: Any]])
            ?? (object["todos"] as? [[String: Any]])
            ?? []
        return tickets.contains { $0["section"] != nil }
    }

    /// The recovery note text, naming the backup path (spec: result `note`).
    static func recoveryNote(backupPath: String) -> String {
        "Previous store was corrupt and backed up to \(backupPath); the store restarted empty."
    }

    // MARK: - Diagnostics (read-only; ADR 0014)

    /// The store's health for `tandem-cli doctor`: the open/done counts, or
    /// "unreadable" for a present-but-invalid file. Unlike `readAll`, this
    /// performs **no recovery** — a corrupt store is reported, never backed up
    /// or rewritten — so a diagnostic never modifies the store. Lock-free, like
    /// every read.
    public enum Health: Sendable, Equatable {
        /// No file yet — a valid empty store (created on first write).
        case absent
        /// A readable store: the load-free counts.
        case counts(open: Int, done: Int)
        /// A file is present but cannot be decoded (corrupt) or read (permission).
        case unreadable
    }

    /// A lock-free, non-mutating health read (see `Health`).
    public func health() -> Health {
        switch peek() {
        case .absent:
            return .absent
        case .valid(let contents, _):
            let done = contents.todos.lazy.filter { $0.done }.count
            return .counts(open: contents.todos.count - done, done: done)
        case .invalid:
            return .unreadable
        }
    }

    /// The rolling backup's state (ADR 0018) for `tandem-cli doctor`: whether
    /// `todos.json.bak` exists, and how long ago it was last written. Separate from
    /// `Health` because it is a property *beside* the store, not of its contents —
    /// and never a failure: a store written once, or read-only, simply has no backup.
    /// Lock-free and non-mutating, like every diagnostic read.
    public enum BackupHealth: Sendable, Equatable {
        /// No `todos.json.bak` — the normal state before the second write.
        case absent
        /// A backup exists; it was written `age` seconds ago.
        case present(age: TimeInterval)
    }

    /// A lock-free read of the rolling backup's presence and age (see `BackupHealth`).
    public func backupHealth() -> BackupHealth {
        guard
            let attrs = try? FileManager.default.attributesOfItem(atPath: backupURL.path),
            let modified = attrs[.modificationDate] as? Date
        else { return .absent }
        return .present(age: Date().timeIntervalSince(modified))
    }

    /// Whether the store's sidecar lock is currently held by another process —
    /// a non-destructive probe (see `LockedDirectory.isLocked`).
    public func lockIsHeld() -> Bool {
        storage.isLocked()
    }
}

/// A domain failure that also triggered corrupt-file recovery — the store's
/// `todos.json` or the sessions file. Carries the underlying failure (for its
/// message and, for `unknownID`, the open list) plus the recovery note, so a
/// caller that both fails and recovers still learns that its prior model is void.
public struct RecoveryFailure: Error, Sendable {
    public let underlying: any TandemError
    public let note: String
}

extension RecoveryFailure: TandemError {
    public var message: String { underlying.message }
}
