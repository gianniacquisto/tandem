import Foundation
import Observation
import AppKit
import TandemCore

/// The observable model for the Bikeshed tab — mirrors `TandemModel`'s shape:
/// a deliberate singleton, everything on the main actor, `private(set)` state,
/// thin non-throwing op wrappers that surface the core's error `message`
/// verbatim, watcher callbacks on main. The `Bikeshed` core type is the ONLY
/// bikeshed logic the GUI holds (spec); the views draw, the model decides.
///
/// Everything runs on the main thread by convention (the `TandemModel`
/// pattern, Swift 5 language mode): UI actions, the watcher callback (main
/// queue), and the debounce/retry task hops (DispatchQueue.main) all land
/// here. The only off-main work is the bikeshed-wide search/backlinks scans,
/// which run in detached tasks and hop their result back to main.
@Observable
final class BikeshedModel {
    static let shared = BikeshedModel()

    /// The only bikeshed logic the GUI holds (a core value; all note semantics —
    /// locking, atomic writes, link rewrite, resolution — live in TandemCore).
    /// Replaced whole by `switchBikeshed(to:)`.
    private(set) var bikeshed: Bikeshed
    private var watcher: BikeshedWatcher

    /// The tickets model — the app's single owner of the store. The bikeshed face
    /// files tickets through it (the "send to tickets" action), so the store has
    /// exactly one owner, as the app's shape intends.
    private let tickets: TandemModel

    // MARK: Tree + selection + search

    /// The bikeshed tree (folders first, alphabetical; `.md` notes only).
    private(set) var tree: [BikeshedNode] = []
    /// Whether the tree reveals the hidden layer — the capture saddlebag
    /// (`.raw/`) and the consolidation archive (`.raw/.done/`). Off by default:
    /// the curated layer is the reading surface. Persisted as a `showHiddenFiles`
    /// row in the store's `settings` table (ADR 0011, re-backed by ADR 0031).
    private(set) var showsHiddenFiles: Bool
    /// The note open in the editor (bikeshed-root-relative path), if any.
    private(set) var openPath: String?
    /// Folders currently expanded in the tree.
    private(set) var expandedFolders: Set<String> = []
    /// Lowercased-path → real-path map (case-insensitive link resolution;
    /// rebuilt with the tree).
    private var pathIndex: [String: String] = [:]
    /// Current bikeshed-wide search results (one row per note, path-sorted).
    private(set) var searchResults: [SearchHit] = []
    /// True while a trimmed non-empty search query is active (results show
    /// in place of the tree).
    private(set) var searchActive = false
    /// The wiki front page (4E5E9A0B): one row per project under `projects/` —
    /// the Overview, its scaffold pages, the "where this left off" line and
    /// the freshness stamp. The no-note-selected home of the tab.
    private(set) var frontPage: [ProjectFront] = []
    /// The most recent operation error, surfaced verbatim (core text).
    private(set) var lastError: String?

    // MARK: Buffer (the open note's document state — the bridge's Swift side)

    /// The open note's raw markdown as it currently sits in the editor buffer.
    /// Raw markdown is the source of truth; the webview decorations are view-only.
    private(set) var bufferText = ""
    /// The on-disk hash the buffer last agreed with (from `read`, or from the
    /// app's own `write` record). Drives conflict detection (content-based;
    /// mtime never involved).
    private var baselineHash: String?
    /// True between a `dirty` message and the save/adopt that resolves it.
    private(set) var isDirty = false
    /// A pending conflict: the buffer is dirty AND the note changed on disk.
    /// The pending autosave is withheld until the user chooses (spec).
    private(set) var conflict = false
    /// The on-disk state at conflict time ("theirs").
    private var theirs: (text: String, hash: String)?
    /// Transient: a core write is in flight (the title bar's "saving…" mark).
    private(set) var saving = false
    /// The app's last write: the watcher dedups its FSEvent by this hash
    /// (a self-write is not an external change).
    private var lastWrite: WriteRecord?

    // MARK: Backlinks (derived state; recomputed off-main, cached per open note)

    private(set) var backlinks: [Backlink] = []

    // MARK: Health (the link-graph report; recomputed off-main, like backlinks)

    /// The bikeshed's link-graph health: the wikilinks that resolve to
    /// nothing and the notes nothing links to. The same core call the CLI's
    /// `note links` makes, so the panel and the command can never disagree.
    private(set) var health: BikeshedHealth = .empty

    // MARK: Editor (the webview lives here — persistent across tab switches)

    let editor: EditorHost

    // Debounce timers (main-queue work items — the model's convention is
    // main-thread-by-design, so no actors/tasks on the hot path) and the two
    // off-main scan tasks (detached; see search/backlinks below).
    private var autosaveTimer: DispatchWorkItem?
    private var saveRetryTimer: DispatchWorkItem?
    private var searchDebounce: DispatchWorkItem?
    private var searchRun: Task<Void, Never>?
    private var backlinkDebounce: DispatchWorkItem?
    private var backlinkRun: Task<Void, Never>?
    private var healthDebounce: DispatchWorkItem?
    private var healthRun: Task<Void, Never>?
    private var frontDebounce: DispatchWorkItem?
    private var frontRun: Task<Void, Never>?

    private init(bikeshed: Bikeshed = Bikeshed(), tickets: TandemModel = .shared) {
        self.bikeshed = bikeshed
        self.tickets = tickets
        self.showsHiddenFiles = bikeshed.showsHiddenFiles
        self.watcher = BikeshedWatcher(directory: bikeshed.directory)
        // The editor host owns the WKWebView and the bridge plumbing (the
        // contract). It is built before `self` is usable (Swift forbids
        // passing `self` to a nested init mid-initialization), so the model
        // links itself in afterwards — the host's first callback arrives
        // asynchronously, well after the attach.
        self.editor = EditorHost()
        self.editor.attach(to: self)
        // Attach the watcher callback only now that `self` is fully
        // initialized (the `StoreWatcher` pattern).
        self.watcher.onEvent = { [weak self] in self?.bikeshedChanged($0) }
        // Cold start reads the tree directly (the spec's full-rescan rule).
        self.reloadTree()
        self.watcher.start()
        // Autosave on window blur (spec): the window losing key status
        // flushes any pending debounce.
        NotificationCenter.default.addObserver(
            forName: NSWindow.didResignKeyNotification,
            object: nil, queue: .main
        ) { [weak self] _ in self?.flushSave() }
    }

    deinit {
        watcher.stop()
        autosaveTimer?.cancel()
        saveRetryTimer?.cancel()
        searchDebounce?.cancel()
        searchRun?.cancel()
        backlinkDebounce?.cancel()
        backlinkRun?.cancel()
        healthDebounce?.cancel()
        healthRun?.cancel()
        frontDebounce?.cancel()
        frontRun?.cancel()
    }
    // (Tasks capture `self` weakly; a deallocated model leaves them no-ops.)

    // MARK: - Bikeshed switch (the human's chosen folder, applied live)

    /// Switch the bikeshed to `url` with no restart: flush the open note,
    /// persist the choice, rebuild the bikeshed and its recursive watcher, and
    /// reset the editor to the no-note-selected state. A no-op when `url` is
    /// already the bikeshed; frozen while a conflict is pending (the banner's
    /// choice must be resolved first — mirroring `open`). A failure to record
    /// the choice surfaces the error and leaves the current bikeshed watching.
    func switchBikeshed(to url: URL) {
        guard !conflict,
              url.standardizedFileURL != bikeshed.directory.standardizedFileURL
        else { return }
        flushSave()
        watcher.stop()
        do {
            try TandemSettings.setChosenBikeshed(url)
        } catch {
            apply(error: error)
            watcher.start()   // the old bikeshed is still in place: keep watching it
            return
        }
        let home = TandemHome.directory
        bikeshed = Bikeshed(
            directory: url,
            lockURL: home.appendingPathComponent("bikeshed.lock"),
            showsHiddenFiles: showsHiddenFiles)
        expandedFolders = []
        closeNote(silent: false)
        watcher = BikeshedWatcher(directory: bikeshed.directory)
        watcher.onEvent = { [weak self] in self?.bikeshedChanged($0) }
        reloadTree()
        watcher.start()
        editor.sendNoteIndex(paths: notePaths())
        lastError = nil
    }

    // MARK: - Tree / derived state

    /// Reveal (or hide) the bikeshed's hidden layer — the capture saddlebag
    /// and the consolidation archive — in the tree. The choice is persisted
    /// like every other setting; the tree is redrawn live. The curated graph
    /// never changes: the hidden layer stays out of search, backlinks, and
    /// wikilink resolution in both states.
    func setShowsHiddenFiles(_ shown: Bool) {
        guard shown != showsHiddenFiles else { return }
        do {
            try TandemSettings.setShowHiddenFiles(shown)
            showsHiddenFiles = shown
            bikeshed.showsHiddenFiles = shown
            reloadTree()
            editor.sendNoteIndex(paths: notePaths())
            lastError = nil
        } catch {
            apply(error: error)
        }
    }

    /// Re-reads the bikeshed tree lock-free (core read paths can never observe a
    /// partial file) and refreshes the derived path index. Called at startup,
    /// after every app op, and on every watcher batch.
    func reloadTree() {
        let nodes = (try? bikeshed.list())?.value ?? []
        tree = nodes
        var index: [String: String] = [:]
        Self.indexPaths(nodes, into: &index)
        pathIndex = index
        // A note that vanished from disk (agent delete/rename) is closed —
        // unless the buffer still holds unsaved edits, which are kept (the
        // next autosave re-creates the path; a disk hiccup never loses words).
        //
        // The question is asked of the **disk**, not of the tree: a note in the
        // hidden layer — a capture in the saddlebag — is on disk but never in
        // the tree's index unless the human revealed the hidden layer, and the
        // Sessions face opens exactly those notes.
        if let open = openPath, pathIndex[open.lowercased()] == nil, !noteExists(open) {
            if !isDirty { closeNote(silent: true) }
        }
        refreshHealth()
        refreshFrontPage()
    }

    /// Whether the note is still on disk. The tree's index answers this for a
    /// visible note; a note in the hidden layer is never in the index, so the
    /// disk is what decides.
    private func noteExists(_ path: String) -> Bool {
        guard let url = fileURL(for: path) else { return false }
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir)
        else { return false }
        return !isDir.boolValue
    }

    private static func indexPaths(_ nodes: [BikeshedNode], into index: inout [String: String]) {
        for node in nodes {
            if node.kind == .note { index[node.path.lowercased()] = node.path }
            indexPaths(node.children, into: &index)
        }
    }

    /// The bikeshed's linkable note paths (bikeshed-root-relative, case as on
    /// disk) — the `setNoteIndex` payload. Derived from the tree, minus the
    /// hidden layer: a revealed capture is drawn and openable, but it is
    /// never a link target and never a search hit (the core's rule — the
    /// curated graph is the same in both states).
    func notePaths() -> [String] {
        Array(pathIndex.values)
            .filter { !Bikeshed.isHiddenPath($0) }
            .sorted {
                ($0.lowercased(), $0) < ($1.lowercased(), $1)
            }
    }

    func toggleFolder(_ path: String) {
        if !expandedFolders.insert(path).inserted {
            expandedFolders.remove(path)
        }
    }

    /// The tree's children of one level (`""` = the bikeshed root), folders
    /// first, alphabetical — exactly the core's `list()` order.
    func treeChildren(of path: String) -> [BikeshedNode] {
        if path.isEmpty { return tree }
        guard let node = Self.find(tree, path: path), node.kind == .folder else { return [] }
        return node.children
    }

    private static func find(_ nodes: [BikeshedNode], path: String) -> BikeshedNode? {
        for node in nodes where node.path == path { return node }
        for node in nodes where node.kind == .folder {
            if path.hasPrefix(node.path + "/"), let hit = find(node.children, path: path) {
                return hit
            }
        }
        return nil
    }

    // MARK: - Search (full bikeshed; debounced; off-main — a bikeshed-wide scan
    // reads every note and must never block the UI)

    func setQuery(_ raw: String) {
        let q = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        searchDebounce?.cancel()
        searchRun?.cancel()
        lastSearchQuery = q
        guard !q.isEmpty else {
            searchActive = false
            searchResults = []
            return
        }
        searchActive = true
        // Coalesce keystrokes: the next query cancels this item.
        let item = DispatchWorkItem { [weak self] in self?.runSearch(q) }
        searchDebounce = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2, execute: item)
    }

    private func runSearch(_ q: String) {
        let bikeshed = self.bikeshed
        searchRun = Task.detached(priority: .userInitiated) { [weak self] in
            let hits = (try? bikeshed.search(query: q))?.value ?? []
            guard !Task.isCancelled else { return }
            DispatchQueue.main.async {
                self?.searchResults = hits
            }
        }
    }

    // MARK: - Backlinks (derived; off-main; the bikeshed-wide recompute is the
    // slow one — debounce + background, per the core's scale note)

    func refreshBacklinks() {
        backlinkDebounce?.cancel()
        backlinkRun?.cancel()
        guard let open = openPath else {
            backlinks = []
            return
        }
        // The bikeshed-wide recompute is slow; debounce it, and skip the result
        // if the open note switched in the meantime.
        let item = DispatchWorkItem { [weak self] in
            guard let self, self.openPath == open else { return }
            self.runBacklinks(open)
        }
        backlinkDebounce = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: item)
    }

    private func runBacklinks(_ open: String) {
        let bikeshed = self.bikeshed
        backlinkRun = Task.detached(priority: .userInitiated) { [weak self] in
            let rows = (try? bikeshed.backlinks(path: open))?.value ?? []
            guard !Task.isCancelled else { return }   // superseded by a newer pass
            DispatchQueue.main.async {
                // A stale result (the note switched or the task was superseded)
                // must not overwrite a fresher one.
                guard let self, self.openPath == open else { return }
                self.backlinks = rows
            }
        }
    }

    // MARK: - Health (derived; off-main; the same bikeshed-wide scan the
    // backlinks path makes, so it is debounced longer — a health report is
    // not a live view, and a typing burst must not cost a scan per keystroke)

    /// Recompute the link-graph report (broken links, orphan notes). Debounced
    /// and off-main: the scan reads every note, so it never runs on the UI
    /// thread.
    func refreshHealth() {
        healthDebounce?.cancel()
        healthRun?.cancel()
        let item = DispatchWorkItem { [weak self] in self?.runHealth() }
        healthDebounce = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0, execute: item)
    }

    private func runHealth() {
        let bikeshed = self.bikeshed
        healthRun = Task.detached(priority: .utility) { [weak self] in
            let report = (try? bikeshed.health())?.value ?? .empty
            guard !Task.isCancelled else { return }   // superseded by a newer pass
            DispatchQueue.main.async {
                self?.health = report
            }
        }
    }

    // MARK: - Front page (derived; off-main; the same reload cadence as the
    // health report — the watcher's batch recompute, debounced)

    /// Recompute the wiki front page (one row per project). Debounced and
    /// off-main like the health report: the derivation reads the projects'
    /// Overview notes, and a watcher burst must not cost a rescan per event.
    func refreshFrontPage() {
        frontDebounce?.cancel()
        frontRun?.cancel()
        let item = DispatchWorkItem { [weak self] in self?.runFrontPage() }
        frontDebounce = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5, execute: item)
    }

    private func runFrontPage() {
        let bikeshed = self.bikeshed
        frontRun = Task.detached(priority: .utility) { [weak self] in
            let rows = (try? bikeshed.frontPage())?.value ?? []
            guard !Task.isCancelled else { return }   // superseded by a newer pass
            DispatchQueue.main.async {
                self?.frontPage = rows
            }
        }
    }

    /// The project row the given open note belongs to, if any (the note sits
    /// inside the project's folder). Drives the editor column's scaffold strip.
    func projectFront(for path: String) -> ProjectFront? {
        frontPage.first { path.hasPrefix($0.folder + "/") }
    }

    // MARK: - Notes: open / create / rename / delete

    /// Opens a note in the editor: flushes the current note's pending save
    /// first (autosave on note switch), then loads the on-disk state into the
    /// buffer and hands it to the webview.
    func open(_ path: String) {
        // The bikeshed is frozen while a conflict is pending (the banner offers
        // the only way out — switching would silently discard the choice).
        guard !conflict, openPath != path else { return }
        flushSave()
        do {
            let content = try bikeshed.read(path: path).value
            bufferText = content.text
            baselineHash = content.hash
            isDirty = false
            conflict = false
            theirs = nil
            lastWrite = nil
            openPath = path
            editor.sendDocument(bufferText, notePath: path)
            editor.sendNoteIndex(paths: notePaths())
            // The human asked for this note — put the caret in it, so typing
            // works without a click into the pane first.
            editor.focusEditor()
            lastError = nil
            refreshBacklinks()
        } catch {
            apply(error: error)
        }
    }

    /// Closes the open note (the face shows the designed no-note-selected
    /// state). `silent` = no error even when the buffer was dirty and had no
    /// note to save into (the note vanished from disk).
    func closeNote(silent: Bool) {
        flushSave()
        openPath = nil
        bufferText = ""
        baselineHash = nil
        isDirty = false
        conflict = false
        theirs = nil
        lastWrite = nil
        backlinks = []
        if !silent { editor.sendDocument("", notePath: "") }
        refreshBacklinks()
    }

    /// "New note": an empty note at the bikeshed root (the spec's one-click
    /// capture) with a collision-free name, then open it. `in:` creates inside
    /// a folder (the tree's folder-row action; user story 5).
    func createNote(in folder: String? = nil) {
        guard !conflict else { return }
        let prefix = folder.map { $0 + "/" } ?? ""
        let taken = Set(notePaths().map { $0.lowercased() })
        var name = prefix + "Untitled.md"
        var n = 2
        while taken.contains(name.lowercased()) {
            name = prefix + "Untitled \(n).md"
            n += 1
        }
        do {
            _ = try bikeshed.create(path: name).value
            lastError = nil
            reloadTree()
            open(name)
        } catch {
            apply(error: error)
        }
    }

    /// "New folder": an empty folder inside `parent` (`nil` / `""` = the bikeshed
    /// root) with a collision-free name, then expand `parent` so it shows.
    /// Mirrors `createNote(in:)`. Folder rename/move is out of scope — a note
    /// moves via the Rename… sheet (rename IS the path change); an empty
    /// folder is harmless to wikilink resolution (it resolves by basename).
    func createFolder(in parent: String? = nil) {
        guard !conflict else { return }
        let prefix = parent.map { $0 + "/" } ?? ""
        let taken = Set(treeChildren(of: parent ?? "").map { $0.name.lowercased() })
        var name = "New Folder"
        var n = 2
        while taken.contains(name.lowercased()) {
            name = "New Folder \(n)"
            n += 1
        }
        do {
            _ = try bikeshed.createFolder(path: prefix + name).value
            lastError = nil
            reloadTree()
            if let parent, !parent.isEmpty { expandedFolders.insert(parent) }
        } catch {
            apply(error: error)
        }
    }

    /// Renames/moves a note — rename IS the path change (spec): the core
    /// rewrites every resolving wikilink in one locked transaction, then moves
    /// the file. A taken target or a bad name is refused by the core with
    /// nothing written; the message surfaces verbatim.
    func renameNote(from oldPath: String, to newPath: String) {
        guard !conflict else { return }
        let trimmed = newPath.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        do {
            _ = try bikeshed.rename(from: oldPath, to: trimmed).value
            lastError = nil
            if openPath == oldPath {
                // The open note moved: the path (and thus relative image
                // targets) changed, so hand the document over again.
                openPath = trimmed
                editor.sendDocument(bufferText, notePath: trimmed)
            }
            reloadTree()
            editor.sendNoteIndex(paths: notePaths())
            refreshBacklinks()
        } catch {
            apply(error: error)
        }
    }

    /// Moves a note or a folder into another folder — the tree's
    /// drag-to-re-parent. The core does the work: it rewrites every wikilink
    /// that resolves to a moved note (a folder move: every note inside it)
    /// and moves the file or the folder in one locked transaction. A cycle
    /// (a folder into its own descendant) and a name already taken in the
    /// destination are refused by the core with nothing written; the message
    /// surfaces verbatim.
    ///
    /// The open note's pending edits are saved first: the move rewrites the
    /// note's own text on disk, and a stale buffer would overwrite that
    /// rewrite on the next save. A failed save aborts the move (the error is
    /// already surfaced).
    func move(_ path: String, into folder: String) {
        guard !conflict else { return }
        flushSave()
        guard !isDirty else { return }   // the save failed; lastError is set
        do {
            let outcome = try bikeshed.move(path: path, into: folder).value
            lastError = nil
            if let open = openPath,
               let moved = outcome.movedNotes.first(where: { $0.oldPath == open })
            {
                // The open note moved: the path (and thus relative image
                // targets) changed, so hand the document over again — with
                // the text the move rewrote, not the buffer's older copy.
                openPath = moved.newPath
                if let content = try? bikeshed.read(path: moved.newPath).value {
                    bufferText = content.text
                    baselineHash = content.hash
                }
                editor.sendDocument(bufferText, notePath: moved.newPath)
            }
            remapExpandedFolders(outcome)
            reloadTree()
            editor.sendNoteIndex(paths: notePaths())
            refreshBacklinks()
        } catch {
            apply(error: error)
        }
    }

    /// A moved folder carries its expanded state with it: the paths of the
    /// expanded folders inside it change with the move, and the tree's
    /// expansion is keyed by path.
    private func remapExpandedFolders(_ outcome: MoveOutcome) {
        guard outcome.isFolder, outcome.oldPath != outcome.path else { return }
        let old = outcome.oldPath
        let new = outcome.path
        expandedFolders = Set(expandedFolders.map { path in
            if path == old { return new }
            if path.hasPrefix(old + "/") { return new + String(path.dropFirst(old.count)) }
            return path
        })
    }

    /// Deletion (note or folder): the macOS Trash first (recoverable — user
    /// story 7), falling back to the core's permanent delete. No confirm
    /// beyond the standard (the row's destructive context menu). If the open
    /// note is the target (or inside the deleted folder), its pending edits
    /// are saved first — a delete never discards unsaved work, and a failed
    /// save aborts the delete (the error is already surfaced).
    func delete(_ path: String) {
        guard !conflict else { return }
        guard let url = fileURL(for: path) else { return }
        if let open = openPath, open == path || open.hasPrefix(path + "/") {
            flushSave()
            guard !isDirty else { return }   // the save failed; lastError set
        }
        var resulting: NSURL?
        do {
            try FileManager.default.trashItem(at: url, resultingItemURL: &resulting)
            removed(path)
        } catch {
            // Trash failed (e.g. the item left the bikeshed in the race): fall
            // back to the core's permanent delete.
            do {
                _ = try bikeshed.delete(path: path).value
                removed(path)
            } catch {
                apply(error: error)
            }
        }
    }

    /// Post-deletion refresh: close the note if the removal covered it,
    /// rebuild the tree, re-send the note index, recompute backlinks.
    private func removed(_ path: String) {
        if let open = openPath, open == path || open.hasPrefix(path + "/") {
            openPath = nil
            bufferText = ""
            baselineHash = nil
            isDirty = false
            conflict = false
            theirs = nil
            lastWrite = nil
            backlinks = []
            editor.sendDocument("", notePath: "")
        }
        lastError = nil
        reloadTree()
        editor.sendNoteIndex(paths: notePaths())
        refreshBacklinks()
    }

    /// The bikeshed-root-relative path as a file URL, or nil for a bikeshed escape
    /// (absolute / `.` / `..` components) — the shared rule (`BikeshedPath`),
    /// which the `tandem:` scheme handler also serves.
    private func fileURL(for path: String) -> URL? {
        BikeshedPath.url(under: bikeshed.directory, for: path)
    }

    // MARK: - Editor bridge — inbound (JS → Swift; the contract's messages)

    /// `loaded`: the page is ready (after DOM + bundle init). Per the
    /// contract, Swift now sends `setNoteIndex`, and `setDocument` if a note
    /// is already selected.
    func editorLoaded() {
        editor.sendNoteIndex(paths: notePaths())
        if let path = openPath {
            editor.sendDocument(bufferText, notePath: path)
            // The note was selected before the page finished loading (the
            // usual cold-start path) — the caret belongs in it.
            editor.focusEditor()
        }
    }

    /// `dirty`: any document change (every change, including checkbox toggles)
    /// arrives with the FULL raw markdown source. Record it, mark the buffer
    /// dirty, and debounce the autosave (~1 s).
    func editorDirty(text: String) {
        guard openPath != nil else { return }
        bufferText = text
        isDirty = true
        scheduleAutosave()
    }

    /// `link-opened`: a wikilink click (target = the raw target exactly as
    /// written — Swift strips fragment/alias per the wikilink grammar) or a
    /// markdown link click (target = the href). The bridge's payload is the
    /// same either way, so the routing is one decision tree: the wikilink
    /// exactly-one rule decides first, then the spec's markdown-link path
    /// existence, then broken-link creation (spec: "Broken links" + the
    /// rendered feature list's markdown-link rules). The webview never
    /// navigates; routing is the Swift side's job.
    func linkOpened(target: String) {
        let t = target.trimmingCharacters(in: .whitespaces)
        guard !t.isEmpty else { return }

        // 1. URLs with a scheme (http(s), mailto, …) → the OS default handler.
        if t.contains("://") {
            if let url = URL(string: t) { NSWorkspace.shared.open(url) }
            return
        }
        // 2. A bare `#fragment` (in-page anchor) is inert in v1.
        if t.hasPrefix("#") { return }

        // 3. Wikilink resolution — the core's exactly-one rule is the
        //    wikilink router (parse per the grammar; fragment/alias stripped).
        //    A resolved link opens the note. (A markdown href to a note by
        //    path lands here too — same note, same result.)
        let link = Wikilink(raw: t)
        if let link,
           case let .resolved(path) = bikeshed.resolve(link.target)
        {
            open(path)
            return
        }

        // 4. Path existence (the markdown-link rule): a target that exists in
        //    the bikeshed by case-insensitive PATH — a path, not a name,
        //    deliberately distinct from the wikilink exactly-one rule — is
        //    internal: a note opens in the editor, an attachment in the OS
        //    default app. This is what a markdown href like `report.pdf`
        //    (wikilink-parseable, but it names an attachment, not a note)
        //    must do, and path-form targets the grammar rejects (a leading
        //    `/`) reach here too.
        if let real = existingFile(for: link?.target ?? t) {
            route(to: real)
            return
        }

        // 5. A broken WIKILINK: create an empty note at the link's path
        //    (bare → bikeshed root; qualified → that path, folders created as
        //    needed by the core) and open it. No confirm dialog — and no
        //    other fall-through: a broken bare link always lands at the
        //    bikeshed root, never a same-named note in some other folder
        //    (the exactly-one rule broke it; reopening one of the
        //    duplicates would be the silent choice the spec forbids).
        if let link {
            createAndOpen(link.target)
            return
        }

        // 6. A non-wikilink target that exists nowhere in the bikeshed → the
        //    OS default handler (spec: everything else).
        if let url = URL(string: t) { NSWorkspace.shared.open(url) }
    }

    /// The spec's markdown-link existence rule: the target as a bikeshed-root-
    /// relative PATH (a leading `/` is stripped) that exists in the bikeshed
    /// case-insensitively — a path, not a name, deliberately distinct from
    /// the wikilink exactly-one name resolution. Every component is matched
    /// against the directory entries case-insensitively (FileManager,
    /// relative to `Bikeshed.directory`), so a case-variant href still finds
    /// the file and the result is the real on-disk path. A directory (or a
    /// path with `.`/`..`/empty components) is not a file: nil.
    private func existingFile(for target: String) -> String? {
        let candidate = target.hasPrefix("/") ? String(target.dropFirst()) : target
        // The shared hygiene rule (`BikeshedPath`, ADR 0008) rejects escapes
        // (empty / `.` / `..` components) before the walk below.
        guard BikeshedPath.url(under: bikeshed.directory, for: candidate) != nil else { return nil }
        let components = candidate
            .split(separator: "/", omittingEmptySubsequences: false)
            .map(String.init)
        var url = bikeshed.directory.standardizedFileURL
        var resolved: [String] = []
        for c in components {
            guard
                let entries = try? FileManager.default.contentsOfDirectory(
                    at: url, includingPropertiesForKeys: nil),
                let match = entries.first(where: {
                    $0.lastPathComponent.lowercased() == c.lowercased()
                })
            else { return nil }
            url = match
            resolved.append(match.lastPathComponent)
        }
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir),
              !isDir.boolValue
        else { return nil }
        return resolved.joined(separator: "/")
    }

    /// Routes to a real bikeshed path: a note opens in the editor; an attachment
    /// (non-`.md`) opens in the OS default app.
    private func route(to realPath: String) {
        if realPath.lowercased().hasSuffix(".md") {
            open(realPath)
        } else {
            if let url = fileURL(for: realPath) { NSWorkspace.shared.open(url) }
        }
    }

    /// A broken link's click: `create` an empty note at the link's path
    /// (bare → bikeshed root; qualified → that path; the core creates parent
    /// folders) and open it. A target that already exists at that exact path
    /// is opened instead (never overwritten).
    private func createAndOpen(_ target: String) {
        let path = target.hasSuffix(".md") ? target : target + ".md"
        if let real = pathIndex[path.lowercased()] {
            open(real)
            return
        }
        do {
            _ = try bikeshed.create(path: path).value
            lastError = nil
            reloadTree()
            open(path)
        } catch {
            apply(error: error)
        }
    }

    // MARK: - Send to tickets (the reverse of the ticket→note link)

    /// The editor's "send to tickets" action: the text it would file (the
    /// selection, or the caret's checkbox line) becomes a ticket in the tickets
    /// list, linked back to the open note. **The note is not modified.**
    ///
    /// The trigger is the editor's — the ⌘⇧T shortcut posts the text, and the
    /// menu item asks the page for it — but the filing is one core operation,
    /// so the two cannot drift. An empty selection is rejected by the core, and
    /// its message surfaces here, in the face the action was taken in.
    func sendToTickets(text: String) {
        guard let path = openPath else {
            lastError = "Open a note first — a ticket needs the note it came from."
            return
        }
        if let message = tickets.addFromNote(text, notePath: path) {
            lastError = message
        } else {
            lastError = nil
        }
    }

    /// The menu item's path: ask the page for the text it would file. The page
    /// answers with the same message the ⌘⇧T shortcut posts.
    func requestSendToTickets() {
        editor.requestTicketCandidate()
    }

    // MARK: - Autosave (debounced ~1 s; on note switch, window blur, ⌘S)

    func scheduleAutosave() {
        autosaveTimer?.cancel()
        let item = DispatchWorkItem { [weak self] in self?.performSave() }
        autosaveTimer = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0, execute: item)   // ~1 s (spec)
    }

    /// Flushes any pending debounce and saves now (⌘S, note switch, window
    /// blur). A failed write keeps the buffer, surfaces the error, and
    /// retries (~2 s later while still dirty).
    func flushSave() {
        autosaveTimer?.cancel()
        performSave()
    }

    private func performSave() {
        guard let path = openPath, isDirty, !conflict, !saving else { return }
        saving = true
        // Conflict detection: the buffer tracks the on-disk hash from `read`;
        // a changed hash means an external write landed.
        if let onDisk = try? bikeshed.read(path: path).value,
           let baseline = baselineHash,
           onDisk.hash != baseline {
            saving = false
            // Dirty buffer + external change → never clobber: the banner
            // offers [Overwrite with mine] / [Take theirs]; the pending
            // autosave is withheld until the choice (conflict gates it).
            conflict = true
            theirs = (text: onDisk.text, hash: onDisk.hash)
            return
        }
        do {
            let record = try bikeshed.write(path: path, text: bufferText).value
            lastWrite = record
            baselineHash = record.hash
            isDirty = false
            saving = false
            lastError = nil
            // The write itself fires the watcher ~0.2 s later; the recorded
            // hash dedups it in bikeshedChanged.
        } catch {
            saving = false
            // Keep the buffer, surface the error, retry.
            apply(error: error)
            saveRetryTimer?.cancel()
            let item = DispatchWorkItem { [weak self] in self?.performSave() }
            saveRetryTimer = item
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.0, execute: item)
        }
    }

    // MARK: - Conflict choices (the banner's two buttons)

    /// "Overwrite with mine": write the buffer over the external change.
    func conflictChooseMine() {
        guard conflict else { return }
        conflict = false
        theirs = nil
        performSave()
    }

    /// "Take theirs": adopt the on-disk state; the buffer's edits are dropped
    /// (the deliberate choice).
    func conflictChooseTheirs() {
        guard conflict, let theirs else { return }
        bufferText = theirs.text
        baselineHash = theirs.hash
        isDirty = false
        conflict = false
        self.theirs = nil
        if let path = openPath {
            editor.sendDocument(theirs.text, notePath: path)
        }
        refreshBacklinks()
    }

    // MARK: - Watcher (one "bikeshed changed" pass per FSEvents batch)

    /// The coalesced batch from `BikeshedWatcher` (main queue). Rebuild the
    /// tree; reload the open note only if its path is in the batch (or the
    /// root itself changed); self-writes dedup by the recorded hash.
    func bikeshedChanged(_ change: BikeshedWatcher.Change) {
        reloadTree()
        // Bridge contract: `setNoteIndex` is sent "on load and after every
        // bikeshed change" — a broken link in the open note starts resolving
        // the moment its target appears, without reopening.
        editor.sendNoteIndex(paths: notePaths())
        refreshBacklinks()
        if searchActive { runSearch(lastSearchQuery) }

        guard let open = openPath else { return }
        let inBatch = change.rootChanged || change.paths.contains(open)
        guard inBatch else { return }

        // Self-write dedup: an FSEvent for our own atomic write whose
        // on-disk hash equals the hash we just wrote is not an external change.
        if let record = lastWrite, record.path == open,
           let onDisk = try? bikeshed.read(path: open).value,
           onDisk.hash == record.hash {
            lastWrite = nil
            return
        }

        guard let onDisk = try? bikeshed.read(path: open).value else {
            // The note vanished (agent delete/rename). Clean buffer → close;
            // dirty buffer → keep it (reloadTree already decided: the next
            // autosave re-creates the path).
            if !isDirty { closeNote(silent: true) }
            return
        }

        if isDirty {
            // Dirty buffer + external change → conflict; the pending
            // autosave is withheld (performSave is gated on !conflict).
            conflict = true
            theirs = (text: onDisk.text, hash: onDisk.hash)
        } else {
            // Clean buffer → adopt the external change silently (spec).
            bufferText = onDisk.text
            baselineHash = onDisk.hash
            theirs = nil
            conflict = false
            editor.sendDocument(onDisk.text, notePath: open)
        }
    }

    /// The active (trimmed) search query, re-run on bikeshed change so live
    /// results stay current while the field has a query.
    private var lastSearchQuery = ""

    // MARK: - Errors (the TandemModel convention: core text, verbatim)

    func dismissError() { lastError = nil }

    private func apply(error: any Error) {
        lastError = Self.message(for: error)
    }

    private static func message(for error: any Error) -> String {
        if let tandem = error as? any TandemError { return tandem.message }
        return error.localizedDescription
    }
}
