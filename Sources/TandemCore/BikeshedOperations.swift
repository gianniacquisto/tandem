import Foundation

/// Contract-level bikeshed note operations (spec: "Bikeshed core —
/// operations"). These are the only public surface the GUI uses; they define
/// the shared semantics so the app and any future agent-facing surface behave
/// identically. Every operation returns `OperationResult<T>` (the Store
/// shape); domain failures are thrown `BikeshedError`s, I/O failures
/// `BikeshedIOError`.
///
/// All app writes run under the exclusive sidecar lock with orphan cleanup
/// first (ADR 0004); readers take no lock.
extension Bikeshed {

    // MARK: - list

    /// `list()`: the vault tree — nested folders + `.md` notes, folders
    /// first, alphabetical within each level. Hidden files, non-`.md`
    /// attachments, and the lockfile (outside the vault) are excluded. A
    /// missing vault folder is an empty tree, never an error. Lock-free.
    public func list() throws -> OperationResult<[VaultNode]> {
        return OperationResult(tree())
    }

    // MARK: - read

    /// `read(path)`: the note's text plus its on-disk hash (SHA-256 of the
    /// bytes; drives conflict detection and self-write dedup) and mtime
    /// (internal truth, not displayed). Lock-free. A missing path throws
    /// `BikeshedError.unknownNote`.
    public func read(path: String) throws -> OperationResult<NoteContent> {
        return OperationResult(try readNote(path))
    }

    // MARK: - create

    /// `create(path)`: a new empty note. The name is validated before the
    /// lock (no `#` / `|` / malformed `/`, no hidden components; note names
    /// end in `.md`; spaces allowed); parent folders are created. A target
    /// that is already taken — the note itself, or an ancestor that would
    /// have to be a folder — is an error, not an overwrite.
    public func create(path: String) throws -> OperationResult<String> {
        try validateNewName(path)   // validation throws before the lock
        guard let url = resolve(path) else {
            throw BikeshedError.invalidName(name: path)   // unreachable: validated above
        }
        try locked {
            if let taken = fileCollision(in: path, includingNote: true) {
                throw BikeshedError.alreadyExists(path: taken)
            }
            try Self.atomicWrite(Data(), to: url)
        }
        return OperationResult(path)
    }

    // MARK: - write

    /// `write(path, text)`: the autosave path — an atomic write under the
    /// vault lock, storing exactly the bytes given (the core never parses
    /// note text; frontmatter passes through untouched). Creates parent
    /// folders and, for a not-yet-existing path, applies the full naming
    /// validation; an existing path (including pre-existing name violators)
    /// is edited in place. Returns what was written — path + hash — so the
    /// watcher can dedup self-writes. A failed write is an error; keeping the
    /// buffer is the model's problem, not the core's.
    public func write(path: String, text: String) throws -> OperationResult<WriteRecord> {
        guard let url = resolve(path) else {
            throw BikeshedError.invalidName(name: path)
        }
        var isDir: ObjCBool = false
        let exists = FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir)
        if !exists {
            try validateNewName(path)   // a new note gets the full naming rules
        } else if isDir.boolValue {
            throw BikeshedIOError.io("path \"\(path)\" is a folder")
        }
        let data = Data(text.utf8)
        try locked {
            // An ancestor that is a file would need to be a folder: the path
            // is uncreatable, so refuse it (the note itself is the overwrite
            // target, not a collision).
            if let taken = fileCollision(in: path, includingNote: false) {
                throw BikeshedError.alreadyExists(path: taken)
            }
            try Self.atomicWrite(data, to: url)
        }
        return OperationResult(WriteRecord(path: path, hash: Self.hash(of: data)))
    }

    // MARK: - delete

    /// `delete(path)`: removes the note — or folder, recursively — from the
    /// vault under the lock. A permanent primitive that rewrites nothing:
    /// links to a deleted note become broken (no tombstones, ADR 0004). The
    /// GUI performs the macOS Trash move (recoverable) ahead of this; this is
    /// the fallback the Trash move falls back to. A missing path throws
    /// `BikeshedError.unknownNote`.
    public func delete(path: String) throws -> OperationResult<Void> {
        guard let url = resolve(path) else {
            throw BikeshedError.invalidName(name: path)
        }
        try locked {
            var isDir: ObjCBool = false
            guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) else {
                throw BikeshedError.unknownNote(path: path)
            }
            do {
                try FileManager.default.removeItem(at: url)
            } catch {
                throw BikeshedIOError.io("could not delete \"\(path)\": \(error)")
            }
        }
        return OperationResult(())
    }
}
