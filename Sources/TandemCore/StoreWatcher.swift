import Foundation
import Darwin

/// Watches a directory for changes via a `DispatchSource` on a file descriptor,
/// invoking `onEvent` on the main queue when anything in it changes. The GUI uses
/// this to refresh when another process (the CLI) writes the store.
///
/// It lives in `TandemCore` — Foundation and Darwin only — so the seam that pins
/// it is `TandemCoreTests`: ADR 0009 rejects a second test target against an
/// executable's top-level code, and a descriptor-level test is the only way to
/// pin the journal append the directory watch cannot see.
///
/// It watches **two** paths, because the store has two ways of changing:
///
/// - The **directory**, for entry changes — a file created, renamed or removed.
///   That is the store's own entry changes (`tandem.db` created, the rolling
///   backup's temp renamed over `tandem.db.bak`, a legacy file copied aside), and
///   it is also what tells the watcher that `tandem.db-wal` was created or removed.
/// - The **WAL file** (`tandem.db-wal`, beside `tandem.db`), for content changes.
///   A SQLite write appends to the existing journal inode and a directory
///   descriptor does not see a regular file's content change — an append that
///   commits after the directory events from the store's rolling backup would
///   otherwise refresh the window at the *pre-commit* snapshot. (Measured: five
///   content modifications of an existing file → zero directory events; one new
///   entry → two.)
///
/// The WAL descriptor is re-established when the journal is created, renamed or
/// removed — SQLite may delete it on the last clean close, and a later write
/// creates a new inode — so the directory watch is both a change signal and the
/// hint to re-attach. A home with no database yet has no WAL to watch: the
/// directory watch alone is exactly the pre-SQLite behaviour (the JSON fallback
/// stays live until the import commits).
public final class StoreWatcher {
    private let directory: URL
    private let walURL: URL
    /// The queue both handlers run on. The app passes the default (`main`) — the
    /// model mutates on the main thread, and that is the owner's contract. Tests
    /// hand in their own serial queue, because a test process has no main run loop
    /// to pump.
    private let queue: DispatchQueue
    /// Set after construction (so the owner can attach a `[weak self]` callback once
    /// it is fully initialized).
    public var onEvent: (() -> Void)?
    private var directorySource: DispatchSourceFileSystemObject?
    private var walSource: DispatchSourceFileSystemObject?
    /// The WAL descriptor the source is on, kept apart from its source so
    /// `refreshWALWatch` can ask whether the path still names that inode.
    private var walDescriptor: Int32 = -1

    public init(directory: URL, queue: DispatchQueue = .main) {
        self.directory = directory
        self.walURL = Self.walURL(in: directory)
        self.queue = queue
    }

    /// The journal the store writes through, beside the database (`tandem.db`).
    /// The store's own path, restated here because the app must not reach into
    /// the core's storage to know what to watch.
    public static func walURL(in directory: URL) -> URL {
        directory.appendingPathComponent("tandem.db-wal")
    }

    /// The paths a watcher on `directory` observes: the directory itself, then
    /// the database's journal. Pinned by `StoreWatcherTests` so the watch set
    /// cannot drift from the store's file names.
    public static func watchedPaths(in directory: URL) -> [URL] {
        [directory, walURL(in: directory)]
    }

    public func start() {
        guard directorySource == nil else { return }
        // O_EVTONLY: a read-only descriptor used purely for event observation.
        let fd = open(directory.path, O_EVTONLY)
        guard fd >= 0 else { return }

        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd,
            eventMask: [.write, .rename, .delete],
            queue: queue  // the handler may mutate the model: main in the app
        )
        source.setEventHandler { [weak self] in
            guard let self else { return }
            // An entry changed: the journal may have just been created or replaced.
            self.refreshWALWatch()
            self.onEvent?()
        }
        source.setCancelHandler {
            close(fd)  // release the watched descriptor when the source is cancelled
        }
        source.resume()
        self.directorySource = source

        // The journal may already be there (a database this home was already using).
        refreshWALWatch()
    }

    public func stop() {
        directorySource?.cancel()
        directorySource = nil
        // The cancel handler closes the descriptor; drop the source from the event
        // path first so no late handler re-attaches it.
        walSource?.cancel()
        walSource = nil
        walDescriptor = -1
    }

    deinit {
        directorySource?.cancel()
        walSource?.cancel()
    }

    /// Point the WAL watch at the inode the path names now. A no-op when it already
    /// does, which is the common case (an append never changes the inode); after a
    /// create, rename or remove it closes the old descriptor and opens the new file.
    private func refreshWALWatch() {
        guard directorySource != nil else { return }
        if walDescriptor >= 0, Self.inode(of: walDescriptor) == Self.inode(atPath: walURL.path) {
            return
        }
        walSource?.cancel()
        walSource = nil
        walDescriptor = -1

        let fd = open(walURL.path, O_EVTONLY)
        guard fd >= 0 else { return }  // no journal yet: the directory watch will say when one appears
        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd,
            eventMask: [.write, .extend, .rename, .delete],
            queue: queue
        )
        source.setEventHandler { [weak self] in
            guard let self else { return }
            // A write or a rename/removal: if the inode is gone, the next write's new
            // journal is re-attached here or by the directory handler that follows.
            self.refreshWALWatch()
            self.onEvent?()
        }
        source.setCancelHandler {
            close(fd)
        }
        source.resume()
        walSource = source
        walDescriptor = fd
    }

    /// The `(device, inode)` a descriptor refers to, or `nil` when the `fstat` fails.
    private static func inode(of fd: Int32) -> Inode? {
        var info = stat()
        guard fstat(fd, &info) == 0 else { return nil }
        return Inode(device: info.st_dev, file: info.st_ino)
    }

    /// The `(device, inode)` a path refers to, or `nil` when nothing is there.
    private static func inode(atPath path: String) -> Inode? {
        var info = stat()
        guard stat(path, &info) == 0 else { return nil }
        return Inode(device: info.st_dev, file: info.st_ino)
    }

    private struct Inode: Equatable {
        var device: dev_t
        var file: ino_t
    }
}
