import Foundation
import Darwin

/// Watches the vault root for external changes (agents, Finder, other editors)
/// with one recursive `FSEventStream` (spec: "Watcher").
///
/// Per the spec's behaviour contract:
/// - flags `FileEvents | NoDefer | UseCFTypes | WatchRoot`,
///   `sinceWhen = kFSEventStreamEventIdSinceNow`, latency ≈ 0.2 s (the
///   built-in coalescing folds an atomic write's temp-write + rename pair
///   into one event), scheduled on a **background** dispatch queue;
/// - each batch is coalesced into a single callback delivered on the **main
///   queue** (so the model may mutate its observable state directly),
///   carrying the changed paths (vault-root-relative, deduped) and whether
///   the stream flagged the root itself as changed;
/// - a full rescan happens on `RootChanged`, on stream start, and on app
///   launch — all three reduce to "the model reads the tree directly."
///
/// Self-write dedup (the app's own atomic write re-fires ~0.2 s later) is the
/// model's job: it recognizes the recorded `WriteRecord.hash`. `StoreWatcher`
/// stays as-is for the todos store (different lifecycle, different API).
final class VaultWatcher {
    /// One coalesced "vault changed" pass: the batch's changed paths
    /// (vault-root-relative, deduped) and whether the stream saw `RootChanged`
    /// (the vault dir itself deleted/replaced — force a full rescan).
    struct Change {
        let paths: [String]
        let rootChanged: Bool
    }

    private let directory: URL
    /// Set after construction (so the owner can attach a `[weak self]`
    /// callback once it is fully initialized) — the `StoreWatcher` pattern.
    var onEvent: ((Change) -> Void)?
    private var stream: ConstFSEventStreamRef?
    /// FSEvents must run on a dispatch queue (or a run loop); the spec says
    /// background — the handler hops to main to deliver the coalesced batch.
    private let queue = DispatchQueue(label: "tandem.vault-watcher")

    init(directory: URL) {
        self.directory = directory
    }

    func start() {
        guard stream == nil else { return }
        // A missing vault is an empty vault (created lazily); make sure the
        // watch root exists or the stream would have nothing to watch.
        try? FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true)

        // The context struct's fields are copied into the stream at creation
        // (FSEvents.h), so a local is valid for the stream's lifetime.
        var context = FSEventStreamContext()
        context.info = Unmanaged.passUnretained(self).toOpaque()

        let flags: FSEventStreamCreateFlags = UInt32(
            kFSEventStreamCreateFlagFileEvents
            | kFSEventStreamCreateFlagNoDefer
            | kFSEventStreamCreateFlagUseCFTypes
            | kFSEventStreamCreateFlagWatchRoot)

        let callback: FSEventStreamCallback = { _, clientInfo, numEvents, eventPaths, eventFlags, _ in
            let watcher = Unmanaged<VaultWatcher>.fromOpaque(clientInfo!).takeUnretainedValue()
            watcher.handle(numEvents: numEvents,
                           paths: eventPaths,
                           flags: eventFlags)
        }

        guard let ref = FSEventStreamCreate(
            nil,
            callback,
            &context,
            [directory.path] as CFArray,
            UInt64(kFSEventStreamEventIdSinceNow),
            0.2,                       // latency ≈ 0.2 s (spec)
            flags
        ) else { return }
        self.stream = ref
        FSEventStreamSetDispatchQueue(ref, queue)
        FSEventStreamStart(ref)
    }

    /// Folds one FSEvents batch into a single main-queue "vault changed" pass.
    private func handle(numEvents: Int,
                        paths: UnsafeMutableRawPointer!,
                        flags: UnsafePointer<FSEventStreamEventFlags>) {
        guard let paths else {
            // No paths (e.g. a bare RootChanged): still one coalesced pass.
            DispatchQueue.main.async { [weak self] in
                self?.onEvent?(Change(paths: [], rootChanged: true))
            }
            return
        }
        let items = UnsafeBufferPointer<UnsafeRawPointer>(
            start: paths.assumingMemoryBound(to: UnsafeRawPointer.self),
            count: numEvents)
        var changed = Set<String>()
        var rootChanged = false
        for i in 0..<numEvents {
            if flags[i] & UInt32(kFSEventStreamEventFlagRootChanged) != 0 {
                rootChanged = true
            }
            // With `kFSEventStreamCreateFlagUseCFTypes` each event path is a CFString.
            let item = Unmanaged<CFString>.fromOpaque(items[i]).takeUnretainedValue() as String
            if let rel = relativePath(of: item) { changed.insert(rel) }
        }
        let change = Change(paths: Array(changed), rootChanged: rootChanged)
        DispatchQueue.main.async { [weak self] in
            self?.onEvent?(change)
        }
    }

    /// The vault-root-relative form of an absolute event path, or `nil` for
    /// anything outside the vault (defensive — the watch root covers it only).
    private func relativePath(of absolute: String) -> String? {
        let root = directory.standardizedFileURL.path
        guard absolute.hasPrefix(root) else { return nil }
        var rel = String(absolute.dropFirst(root.count))
        while rel.hasPrefix("/") { rel.removeFirst() }
        return rel.isEmpty ? nil : rel
    }

    func stop() {
        guard let stream else { return }
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        FSEventStreamRelease(stream)
        self.stream = nil
    }

    deinit { stop() }
}
