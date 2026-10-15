import Foundation

/// A node in the vault tree (spec: "list"): a folder or a `.md` note.
/// Hidden files, non-`.md` attachments, and the lockfile (which lives outside
/// the vault) never appear.
public struct VaultNode: Sendable, Equatable {
    public enum Kind: Sendable, Equatable {
        case folder
        case note
    }

    /// The node's name — the last path component, exactly as on disk.
    public let name: String
    /// The vault-root-relative path (`/`-separated; a note's path includes its
    /// `.md` name). This is the path the note operations take.
    public let path: String
    public let kind: Kind
    /// Children, folders first, alphabetical within each level; empty for notes.
    public let children: [VaultNode]

    init(name: String, path: String, kind: Kind, children: [VaultNode]) {
        self.name = name
        self.path = path
        self.kind = kind
        self.children = children
    }
}

/// A note's on-disk state: the text, its content hash, and its mtime.
/// The hash drives conflict detection and self-write dedup (spec: vault
/// protocol — external changes); the mtime is internal truth, not displayed.
public struct NoteContent: Sendable, Equatable {
    public let text: String
    /// SHA-256 of the on-disk bytes, lowercase hex.
    public let hash: String
    /// The file's on-disk modification time.
    public let modifiedAt: Date
}

/// What `write` just put on disk: the path and the hash of the written bytes.
/// The GUI's watcher dedups self-writes with it (an FSEvent on this path whose
/// on-disk hash equals this hash is the app's own write, not an external
/// change).
public struct WriteRecord: Sendable, Equatable {
    public let path: String
    public let hash: String
}
