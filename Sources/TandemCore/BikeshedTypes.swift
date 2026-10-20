import Foundation

/// A node in the bikeshed tree (spec: "list"): a folder or a `.md` note.
/// Hidden files, non-`.md` attachments, and the lockfile (which lives outside
/// the bikeshed) never appear.
public struct BikeshedNode: Sendable, Equatable {
    public enum Kind: Sendable, Equatable {
        case folder
        case note
    }

    /// The node's name — the last path component, exactly as on disk.
    public let name: String
    /// The bikeshed-root-relative path (`/`-separated; a note's path includes its
    /// `.md` name). This is the path the note operations take.
    public let path: String
    public let kind: Kind
    /// Children, folders first, alphabetical within each level; empty for notes.
    public let children: [BikeshedNode]

    init(name: String, path: String, kind: Kind, children: [BikeshedNode]) {
        self.name = name
        self.path = path
        self.kind = kind
        self.children = children
    }
}

/// A note's on-disk state: the text, its content hash, and its mtime.
/// The hash drives conflict detection and self-write dedup (spec: bikeshed
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

/// What a `move` did: the moved item's new path, and every note whose path
/// changed with it (one for a note move; all of them for a folder move).
public struct MoveOutcome: Sendable, Equatable {
    /// The moved item's new bikeshed-root-relative path — a note path (with
    /// its `.md`) or a folder path.
    public let path: String
    /// The moved item's old bikeshed-root-relative path.
    public let oldPath: String
    /// True when the moved item is a folder (its notes moved with it).
    public let isFolder: Bool
    /// The notes whose path changed — a note move carries one, a folder move
    /// every note under it. Empty when the item was already in the
    /// destination folder (the move was a no-op).
    public let movedNotes: [MovedNote]
}

/// One note whose path changed in a move: where it was, where it is now.
public struct MovedNote: Sendable, Equatable {
    /// The note's path before the move.
    public let oldPath: String
    /// The note's path after the move.
    public let newPath: String
}

/// One backlink row (spec: "backlinks(path)"): a bikeshed note that contains at
/// least one wikilink resolving to the target note. Derived state —
/// recomputed from the file set, never stored.
public struct Backlink: Sendable, Equatable {
    /// The linking note's bikeshed-root-relative path.
    public let path: String
    /// The line of the linking note containing the first wikilink that
    /// resolves to the target (without its newline).
    public let line: String
}

/// One search result row (spec: "search(query)"): a note matching the
/// case-insensitive substring query over title (first line) or body (the
/// rest), with the display text — the first line for a title hit, a snippet
/// around the first match for a body hit.
public struct SearchHit: Sendable, Equatable {
    /// The matching note's bikeshed-root-relative path.
    public let path: String
    /// What the result row shows: the note's first line (title hit) or a
    /// whitespace-collapsed snippet around the first body match, ellipsised
    /// at the edges when sliced.
    public let display: String
}
