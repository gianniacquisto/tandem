import Foundation

/// Watches a directory for changes via a `DispatchSource` on the directory's file
/// descriptor, invoking `onEvent` on the main queue when anything in it changes. The
/// GUI uses this to refresh when another process (the CLI) writes the store.
///
/// We watch the *directory*, not the data file: the store writes atomically via
/// `rename`, which replaces the file's inode, so a monitor on the file's fd would keep
/// pointing at the old (renamed-away) inode and miss every update. Watching the
/// directory catches the replace as a change event. (Verified: an atomic
/// `.tmp`-then-`rename` into the directory fires the handler.)
final class StoreWatcher {
    private let directory: URL
    /// Set after construction (so the owner can attach a `[weak self]` callback once
    /// it is fully initialized).
    var onEvent: (() -> Void)?
    private var source: DispatchSourceFileSystemObject?

    init(directory: URL) {
        self.directory = directory
    }

    func start() {
        guard source == nil else { return }
        // O_EVTONLY: a read-only descriptor used purely for event observation.
        let fd = open(directory.path, O_EVTONLY)
        guard fd >= 0 else { return }

        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd,
            eventMask: [.write, .rename, .delete],
            queue: .main  // the handler runs on the main thread, so it may mutate the model
        )
        source.setEventHandler { [weak self] in
            self?.onEvent?()
        }
        source.setCancelHandler {
            close(fd)  // release the watched descriptor when the source is cancelled
        }
        source.resume()
        self.source = source
    }

    func stop() {
        source?.cancel()
        source = nil
    }

    deinit { source?.cancel() }
}
