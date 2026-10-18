import Foundation
import CryptoKit

/// Tandem's notes surface (GLOSSARY: Bikeshed): a plain folder of markdown
/// notes (filename = note identity, nested folders, non-`.md` files are
/// attachments that are invisible in the tree), shared by the app and agents
/// on the same machine. Sibling of `Store`; implements the vault protocol of
/// ADR 0004 and the rename mechanics of ADR 0005:
///
/// - **Identity & naming** — a note is any file ending `.md` (case-
///   insensitive). Hidden files are ignored everywhere; non-`.md` files are
///   attachments, reachable only by relative path from a note. The app only
///   *creates* names free of `#` and `|` (the wikilink grammar's separators);
///   pre-existing violators on disk are still shown and edited.
/// - **Frontmatter** — tolerated, opaque: the core never parses note text.
/// - **Locking** — app writes take an exclusive `flock` on the sidecar
///   lockfile (`<home>/bikeshed.lock`, in Tandem's home and outside the vault
///   even when the vault is a chosen folder elsewhere — ADR 0011 — never
///   renamed, the ADR 0001 sidecar pattern), held across the whole operation,
///   multi-file operations included. Readers take no lock: atomic rename
///   guarantees a reader sees a complete old or new file. The lock is
///   one-sided by design — agents and external editors bypass it (ADR 0004).
/// - **Contention** — bounded retry (10–50 ms backoff, ~2 s), then a clear
///   "vault locked" error.
/// - **Atomic writes** — same-directory temp `.note-<random>.tmp` + fsync +
///   `rename(2)`; orphaned temps older than 60 s are cleaned after acquiring
///   the lock.
/// - **Rename** — one locked transaction: rewrite the referencing notes first
///   (per-file atomic), then a single `rename(2)` of the note file. No
///   journal (ADR 0005).
///
/// A missing vault folder is an empty vault: created lazily, never an error.
/// The public surface is the note operations (see the `BikeshedOperations`
/// extension); the locking/file mechanics are the shared `LockedDirectory`
/// primitive (ADR 0007), which the store uses too.
public struct Bikeshed: Sendable {
    /// The vault root (created lazily). Public so the GUI can watch it for
    /// external changes (FSEvents); the GUI never writes it directly.
    public let directory: URL
    /// The sidecar lockfile, outside the vault (created if missing, never
    /// renamed).
    let lockURL: URL
    /// The shared storage mechanics (ADR 0004, reusing ADR 0001): flock + atomic
    /// write + orphan sweep, identical to the store's.
    private let storage: LockedDirectory

    /// The default vault: the human's chosen folder when one is set
    /// (`TandemSettings.chosenVault()`, ADR 0011), else `<home>/bikeshed`. The
    /// lockfile stays `<home>/bikeshed.lock` (in Tandem's home, never inside
    /// a chosen user folder), where `<home>` is the real `~/.tandem` from the
    /// shipped app and `~/.tandem-dev` from anything built and run in place
    /// (see `TandemHome`).
    public init() {
        let home = TandemHome.directory
        self.init(
            directory: TandemSettings.chosenVault() ?? home.appendingPathComponent("bikeshed"),
            lockURL: home.appendingPathComponent("bikeshed.lock"))
    }

    /// A vault rooted at an explicit directory with an explicit sidecar
    /// lockfile. Used by the default (chosen-vault) path, whose lock lives in
    /// Tandem's home rather than beside an arbitrary user folder.
    public init(directory: URL, lockURL: URL) {
        self.directory = directory
        self.lockURL = lockURL
        self.storage = LockedDirectory(
            lockURL: lockURL,
            tempPrefix: ".note-",
            cleanRoot: directory,
            cleanRecursively: true)
    }

    /// A vault rooted at an explicit directory (used by tests with a temp
    /// dir). The lockfile is the sibling `bikeshed.lock` outside the vault.
    public init(directory: URL) {
        self.init(
            directory: directory,
            lockURL: directory.deletingLastPathComponent()
                .appendingPathComponent("bikeshed.lock"))
    }

    // MARK: - Locked write primitive

    /// Run `body` under the exclusive vault lock (translating the storage
    /// primitive's errors into the vault's own vocabulary): acquire (bounded
    /// retry), clean orphaned temps, then do the work — all in one transaction,
    /// multi-file operations included (ADR 0004). The lock is released when
    /// `body` returns or throws. Internal (not public): the public surface is
    /// the `BikeshedOperations` extension; this is the primitive it builds on.
    func locked<T: Sendable>(_ body: () throws -> T) throws -> T {
        do {
            return try storage.withLock(body)
        } catch LockedDirectoryError.locked {
            throw BikeshedError.locked
        } catch LockedDirectoryError.posix(let op, let code) {
            throw BikeshedIOError.posix(op: op, code: code)
        }
    }

    /// Write `data` to `url` atomically — same-directory temp file, fsync,
    /// `rename(2)` — via the shared storage primitive. Call only while holding
    /// the vault lock.
    func atomicWrite(_ data: Data, to url: URL) throws {
        do {
            try storage.atomicWrite(data, to: url)
        } catch LockedDirectoryError.posix(let op, let code) {
            throw BikeshedIOError.posix(op: op, code: code)
        }
    }

    /// The first component of `path` (root → leaf) that exists as a regular
    /// file — a taken name. For `create`, that is the note itself or an
    /// ancestor that would need to be a folder; for `write` (an overwrite by
    /// definition), only the ancestors are checked. `nil` when nothing
    /// collides. Call while holding the lock.
    func fileCollision(in path: String, includingNote: Bool) -> String? {
        let fm = FileManager.default
        let components = path.split(separator: "/").map(String.init)
        let toCheck = includingNote ? components : Array(components.dropLast())
        var cursor = directory
        var prefix = ""
        for name in toCheck {
            prefix = prefix.isEmpty ? name : prefix + "/" + name
            cursor = cursor.appendingPathComponent(name)
            var isDir: ObjCBool = false
            guard fm.fileExists(atPath: cursor.path, isDirectory: &isDir),
                  !isDir.boolValue
            else { continue }
            return prefix
        }
        return nil
    }

    // MARK: - Vault walk (internal)

    private static func isHidden(_ name: String) -> Bool {
        name.hasPrefix(".")
    }

    private static func isNoteName(_ name: String) -> Bool {
        name.lowercased().hasSuffix(".md")
    }

    /// The sort order within a tree level: alphabetical, case-insensitive,
    /// raw name as the deterministic tie-break.
    private static func sortKey(_ name: String) -> (String, String) {
        (name.lowercased(), name)
    }

    /// The nodes of one directory level: folders first, alphabetical within
    /// each level. Hidden entries are ignored (hidden folders prune their
    /// whole subtree); non-`.md` files are attachments and stay out of the
    /// tree. A missing directory is an empty level — never an error.
    private static func nodes(in dir: URL, relativePath: String) -> [VaultNode] {
        let fm = FileManager.default
        guard
            let entries = try? fm.contentsOfDirectory(
                at: dir, includingPropertiesForKeys: [.isDirectoryKey])
        else {
            return []
        }
        var folders: [VaultNode] = []
        var notes: [VaultNode] = []
        for url in entries {
            let name = url.lastPathComponent
            if isHidden(name) { continue }
            let relative = relativePath.isEmpty ? name : relativePath + "/" + name
            guard
                let values = try? url.resourceValues(forKeys: [.isDirectoryKey]),
                let isDir = values.isDirectory
            else { continue }
            if isDir {
                folders.append(VaultNode(
                    name: name, path: relative, kind: .folder,
                    children: nodes(in: url, relativePath: relative)))
            } else if isNoteName(name) {
                notes.append(VaultNode(name: name, path: relative, kind: .note, children: []))
            }
        }
        folders.sort { sortKey($0.name) < sortKey($1.name) }
        notes.sort { sortKey($0.name) < sortKey($1.name) }
        return folders + notes
    }

    /// The full tree, root level (folders first, alphabetical).
    func tree() -> [VaultNode] {
        Self.nodes(in: directory, relativePath: "")
    }

    /// The vault's note paths (vault-root-relative, original case), recursive.
    /// Hidden files and non-`.md` attachments are excluded; a missing vault
    /// folder is an empty set.
    func notePaths() -> [String] {
        var paths: [String] = []
        func collect(_ nodes: [VaultNode]) {
            for node in nodes {
                if node.kind == .note { paths.append(node.path) }
                collect(node.children)
            }
        }
        collect(Self.nodes(in: directory, relativePath: ""))
        return paths
    }

    // MARK: - Path resolution (internal)

    /// Resolve a vault-root-relative path to a URL inside the vault (the shared
    /// `VaultPath` hygiene rule, ADR 0008): absolute paths, empty components
    /// (`//`), `.` / `..` components (vault escapes), and a missing final
    /// component are rejected. Pre-existing name violators (`#`, `|`) are
    /// *tolerated* here — they are on-disk names the app must still show and
    /// edit; the create/rename validation (`validateNewName`) is the separate,
    /// stricter rule.
    func fileURL(for path: String) -> URL? {
        VaultPath.url(under: directory, for: path)
    }

    /// The naming rules for a path the app is about to create (create / rename
    /// targets / new write paths): path hygiene (no empty, `.`, or `..`
    /// components), no hidden components (hidden files are ignored everywhere
    /// — a hidden note would be created yet never visible), no `#` or `|` in
    /// any component (the wikilink grammar's separators), and the note name
    /// ends in `.md` (case-insensitive). Spaces are allowed.
    func validateNewName(_ path: String) throws {
        let last = try validateComponents(path)
        guard Self.isNoteName(last) else {
            throw BikeshedError.invalidName(name: path)
        }
    }

    /// The naming rules for a *folder* the app is about to create
    /// (`createFolder`): the same path hygiene as `validateNewName` — no
    /// empty / `.` / `..` component, no hidden component, no `#` or `|` — minus
    /// the `.md` requirement (a folder legitimately has no extension). Reuses
    /// the same error.
    func validateNewFolderName(_ path: String) throws {
        _ = try validateComponents(path)
    }

    /// The shared path hygiene for `validateNewName` / `validateNewFolderName`:
    /// no empty, `.`, or `..` component, no hidden component, no `#` or `|` in
    /// any component. Returns the last component so the note rule can add its
    /// `.md` check on top.
    private func validateComponents(_ path: String) throws -> String {
        let components = path.split(separator: "/", omittingEmptySubsequences: false)
            .map(String.init)
        guard let last = components.last, !components.isEmpty else {
            throw BikeshedError.invalidName(name: path)
        }
        for component in components
        where component.isEmpty || component == "." || component == ".."
            || Self.isHidden(component)
            || component.contains("#") || component.contains("|")
        {
            throw BikeshedError.invalidName(name: path)
        }
        return last
    }

    // MARK: - Note read (internal)

    /// Read a note's on-disk state: text, SHA-256 hash of the raw bytes, mtime.
    /// Lock-free (readers take no lock — ADR 0001/0004). The core never parses
    /// the text; it is bytes in, bytes out.
    func readNote(_ path: String) throws -> NoteContent {
        guard let url = fileURL(for: path) else {
            throw BikeshedError.unknownNote(path: path)
        }
        let fm = FileManager.default
        var isDir: ObjCBool = false
        guard fm.fileExists(atPath: url.path, isDirectory: &isDir), !isDir.boolValue else {
            throw BikeshedError.unknownNote(path: path)
        }
        guard let data = try? Data(contentsOf: url) else {
            throw BikeshedIOError.io("could not read \"\(path)\"")
        }
        guard let text = String(data: data, encoding: .utf8) else {
            throw BikeshedIOError.io("note \"\(path)\" is not valid UTF-8")
        }
        guard
            let values = try? url.resourceValues(forKeys: [.contentModificationDateKey]),
            let mod = values.contentModificationDate
        else {
            throw BikeshedIOError.io("could not stat \"\(path)\"")
        }
        return NoteContent(text: text, hash: Self.hash(of: data), modifiedAt: mod)
    }

    /// The content hash: SHA-256 of the bytes, lowercase hex. The same function
    /// hashes what `write` writes, so the GUI can recognize the app's own
    /// writes by the hash it just wrote.
    static func hash(of data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}
