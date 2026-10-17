import Foundation
import Observation
import AppKit
import TandemCore

/// The observable model for the Bikeshed tab — mirrors `TandemModel`'s shape:
/// a deliberate singleton, everything on the main actor, `private(set)` state,
/// thin non-throwing op wrappers that surface the core's error `message`
/// verbatim, watcher callbacks on main. The `Bikeshed` core type is the ONLY
/// vault logic the GUI holds (spec); the views draw, the model decides.
///
/// Everything runs on the main thread by convention (the `TandemModel`
/// pattern, Swift 5 language mode): UI actions, the watcher callback (main
/// queue), and the debounce/retry task hops (DispatchQueue.main) all land
/// here. The only off-main work is the vault-wide search/backlinks scans,
/// which run in detached tasks and hop their result back to main.
@Observable
final class BikeshedModel {
    static let shared = BikeshedModel()

    /// The only vault logic the GUI holds (a core value; all note semantics —
    /// locking, atomic writes, link rewrite, resolution — live in TandemCore).
    let vault: Bikeshed
    private let watcher: VaultWatcher

    // MARK: Tree + selection + search

    /// The vault tree (folders first, alphabetical; `.md` notes only).
    private(set) var tree: [VaultNode] = []
    /// The note open in the editor (vault-root-relative path), if any.
    private(set) var openPath: String?
    /// Folders currently expanded in the tree.
    private(set) var expandedFolders: Set<String> = []
    /// Lowercased-path → real-path map (case-insensitive link resolution;
    /// rebuilt with the tree).
    private var pathIndex: [String: String] = [:]
    /// Current full-vault search results (one row per note, path-sorted).
    private(set) var searchResults: [SearchHit] = []
    /// True while a trimmed non-empty search query is active (results show
    /// in place of the tree).
    private(set) var searchActive = false
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

    private init(vault: Bikeshed = Bikeshed()) {
        self.vault = vault
        self.watcher = VaultWatcher(directory: vault.directory)
        // The editor host owns the WKWebView and the bridge plumbing (the
        // contract). It is built before `self` is usable (Swift forbids
        // passing `self` to a nested init mid-initialization), so the model
        // links itself in afterwards — the host's first callback arrives
        // asynchronously, well after the attach.
        self.editor = EditorHost()
        self.editor.attach(to: self)
        // Attach the watcher callback only now that `self` is fully
        // initialized (the `StoreWatcher` pattern).
        self.watcher.onEvent = { [weak self] in self?.vaultChanged($0) }
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
    }
    // (Tasks capture `self` weakly; a deallocated model leaves them no-ops.)

    // MARK: - Tree / derived state

    /// Re-reads the vault tree lock-free (core read paths can never observe a
    /// partial file) and refreshes the derived path index. Called at startup,
    /// after every app op, and on every watcher batch.
    func reloadTree() {
        let nodes = (try? vault.list())?.value ?? []
        tree = nodes
        var index: [String: String] = [:]
        Self.indexPaths(nodes, into: &index)
        pathIndex = index
        // A note that vanished from disk (agent delete/rename) is closed —
        // unless the buffer still holds unsaved edits, which are kept (the
        // next autosave re-creates the path; a disk hiccup never loses words).
        if let open = openPath, pathIndex[open.lowercased()] == nil {
            if !isDirty { closeNote(silent: true) }
        }
    }

    private static func indexPaths(_ nodes: [VaultNode], into index: inout [String: String]) {
        for node in nodes {
            if node.kind == .note { index[node.path.lowercased()] = node.path }
            indexPaths(node.children, into: &index)
        }
    }

    /// The vault's note paths (vault-root-relative, case as on disk) — the
    /// `setVaultIndex` payload. Derived from the tree (notes only; attachments
    /// are never in it — notes only are linkable).
    func notePaths() -> [String] {
        Array(pathIndex.values).sorted {
            ($0.lowercased(), $0) < ($1.lowercased(), $1)
        }
    }

    func toggleFolder(_ path: String) {
        if !expandedFolders.insert(path).inserted {
            expandedFolders.remove(path)
        }
    }

    /// The tree's children of one level (`""` = the vault root), folders
    /// first, alphabetical — exactly the core's `list()` order.
    func treeChildren(of path: String) -> [VaultNode] {
        if path.isEmpty { return tree }
        guard let node = Self.find(tree, path: path), node.kind == .folder else { return [] }
        return node.children
    }

    private static func find(_ nodes: [VaultNode], path: String) -> VaultNode? {
        for node in nodes where node.path == path { return node }
        for node in nodes where node.kind == .folder {
            if path.hasPrefix(node.path + "/"), let hit = find(node.children, path: path) {
                return hit
            }
        }
        return nil
    }

    // MARK: - Search (full vault; debounced; off-main — a vault-wide scan
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
        let vault = self.vault
        searchRun = Task.detached(priority: .userInitiated) { [weak self] in
            let hits = (try? vault.search(query: q))?.value ?? []
            guard !Task.isCancelled else { return }
            DispatchQueue.main.async {
                self?.searchResults = hits
            }
        }
    }

    // MARK: - Backlinks (derived; off-main; the vault-wide recompute is the
    // slow one — debounce + background, per the core's scale note)

    func refreshBacklinks() {
        backlinkDebounce?.cancel()
        backlinkRun?.cancel()
        guard let open = openPath else {
            backlinks = []
            return
        }
        // The vault-wide recompute is slow; debounce it, and skip the result
        // if the open note switched in the meantime.
        let item = DispatchWorkItem { [weak self] in
            guard let self, self.openPath == open else { return }
            self.runBacklinks(open)
        }
        backlinkDebounce = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3, execute: item)
    }

    private func runBacklinks(_ open: String) {
        let vault = self.vault
        backlinkRun = Task.detached(priority: .userInitiated) { [weak self] in
            let rows = (try? vault.backlinks(path: open))?.value ?? []
            guard !Task.isCancelled else { return }   // superseded by a newer pass
            DispatchQueue.main.async {
                // A stale result (the note switched or the task was superseded)
                // must not overwrite a fresher one.
                guard let self, self.openPath == open else { return }
                self.backlinks = rows
            }
        }
    }

    // MARK: - Notes: open / create / rename / delete

    /// Opens a note in the editor: flushes the current note's pending save
    /// first (autosave on note switch), then loads the on-disk state into the
    /// buffer and hands it to the webview.
    func open(_ path: String) {
        // The vault is frozen while a conflict is pending (the banner offers
        // the only way out — switching would silently discard the choice).
        guard !conflict, openPath != path else { return }
        flushSave()
        do {
            let content = try vault.read(path: path).value
            bufferText = content.text
            baselineHash = content.hash
            isDirty = false
            conflict = false
            theirs = nil
            lastWrite = nil
            openPath = path
            editor.sendDocument(bufferText, notePath: path)
            editor.sendVaultIndex(paths: notePaths())
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

    /// "New note": an empty note at the vault root (the spec's one-click
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
            _ = try vault.create(path: name).value
            lastError = nil
            reloadTree()
            open(name)
        } catch {
            apply(error: error)
        }
    }

    /// "New folder": an empty folder inside `parent` (`nil` / `""` = the vault
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
            _ = try vault.createFolder(path: prefix + name).value
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
            _ = try vault.rename(from: oldPath, to: trimmed).value
            lastError = nil
            if openPath == oldPath {
                // The open note moved: the path (and thus relative image
                // targets) changed, so hand the document over again.
                openPath = trimmed
                editor.sendDocument(bufferText, notePath: trimmed)
            }
            reloadTree()
            editor.sendVaultIndex(paths: notePaths())
            refreshBacklinks()
        } catch {
            apply(error: error)
        }
    }

    /// Deletion (note or folder): the macOS Trash first (recoverable — user
    /// story 7), falling back to the core's permanent delete. No confirm
    /// beyond the standard (the row's destructive context menu). If the open
    /// note is the target (or inside the deleted folder), its pending edits
    /// are saved first — a delete never discards unsaved work, and a failed
    /// save aborts the delete (the error is already surfaced).
    func delete(_ path: String) {
        guard !conflict else { return }
        guard let url = vaultURL(for: path) else { return }
        if let open = openPath, open == path || open.hasPrefix(path + "/") {
            flushSave()
            guard !isDirty else { return }   // the save failed; lastError set
        }
        var resulting: NSURL?
        do {
            try FileManager.default.trashItem(at: url, resultingItemURL: &resulting)
            removed(path)
        } catch {
            // Trash failed (e.g. the item left the vault in the race): fall
            // back to the core's permanent delete.
            do {
                _ = try vault.delete(path: path).value
                removed(path)
            } catch {
                apply(error: error)
            }
        }
    }

    /// Post-deletion refresh: close the note if the removal covered it,
    /// rebuild the tree, re-send the vault index, recompute backlinks.
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
        editor.sendVaultIndex(paths: notePaths())
        refreshBacklinks()
    }

    /// The vault-root-relative path as a file URL, or nil for a vault escape
    /// (absolute / `.` / `..` components) — the shared rule (`VaultPath`),
    /// which the `tandem:` scheme handler also serves.
    private func vaultURL(for path: String) -> URL? {
        VaultPath.url(under: vault.directory, for: path)
    }

    // MARK: - Editor bridge — inbound (JS → Swift; the contract's messages)

    /// `loaded`: the page is ready (after DOM + bundle init). Per the
    /// contract, Swift now sends `setVaultIndex`, and `setDocument` if a note
    /// is already selected.
    func editorLoaded() {
        editor.sendVaultIndex(paths: notePaths())
        if let path = openPath {
            editor.sendDocument(bufferText, notePath: path)
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
           case let .resolved(path) = vault.resolve(link.target)
        {
            open(path)
            return
        }

        // 4. Path existence (the markdown-link rule): a target that exists in
        //    the vault by case-insensitive PATH — a path, not a name,
        //    deliberately distinct from the wikilink exactly-one rule — is
        //    internal: a note opens in the editor, an attachment in the OS
        //    default app. This is what a markdown href like `report.pdf`
        //    (wikilink-parseable, but it names an attachment, not a note)
        //    must do, and path-form targets the grammar rejects (a leading
        //    `/`) reach here too.
        if let real = existingVaultFile(for: link?.target ?? t) {
            route(to: real)
            return
        }

        // 5. A broken WIKILINK: create an empty note at the link's path
        //    (bare → vault root; qualified → that path, folders created as
        //    needed by the core) and open it. No confirm dialog — and no
        //    other fall-through: a broken bare link always lands at the
        //    vault root, never a same-named note in some other folder
        //    (the exactly-one rule broke it; reopening one of the
        //    duplicates would be the silent choice the spec forbids).
        if let link {
            createAndOpen(link.target)
            return
        }

        // 6. A non-wikilink target that exists nowhere in the vault → the
        //    OS default handler (spec: everything else).
        if let url = URL(string: t) { NSWorkspace.shared.open(url) }
    }

    /// The spec's markdown-link existence rule: the target as a vault-root-
    /// relative PATH (a leading `/` is stripped) that exists in the vault
    /// case-insensitively — a path, not a name, deliberately distinct from
    /// the wikilink exactly-one name resolution. Every component is matched
    /// against the directory entries case-insensitively (FileManager,
    /// relative to `Bikeshed.directory`), so a case-variant href still finds
    /// the file and the result is the real on-disk path. A directory (or a
    /// path with `.`/`..`/empty components) is not a file: nil.
    private func existingVaultFile(for target: String) -> String? {
        let candidate = target.hasPrefix("/") ? String(target.dropFirst()) : target
        // The shared hygiene rule (`VaultPath`, ADR 0008) rejects escapes
        // (empty / `.` / `..` components) before the walk below.
        guard VaultPath.url(under: vault.directory, for: candidate) != nil else { return nil }
        let components = candidate
            .split(separator: "/", omittingEmptySubsequences: false)
            .map(String.init)
        var url = vault.directory.standardizedFileURL
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

    /// Routes to a real vault path: a note opens in the editor; an attachment
    /// (non-`.md`) opens in the OS default app.
    private func route(to realPath: String) {
        if realPath.lowercased().hasSuffix(".md") {
            open(realPath)
        } else {
            if let url = vaultURL(for: realPath) { NSWorkspace.shared.open(url) }
        }
    }

    /// A broken link's click: `create` an empty note at the link's path
    /// (bare → vault root; qualified → that path; the core creates parent
    /// folders) and open it. A target that already exists at that exact path
    /// is opened instead (never overwritten).
    private func createAndOpen(_ target: String) {
        let path = target.hasSuffix(".md") ? target : target + ".md"
        if let real = pathIndex[path.lowercased()] {
            open(real)
            return
        }
        do {
            _ = try vault.create(path: path).value
            lastError = nil
            reloadTree()
            open(path)
        } catch {
            apply(error: error)
        }
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
        if let onDisk = try? vault.read(path: path).value,
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
            let record = try vault.write(path: path, text: bufferText).value
            lastWrite = record
            baselineHash = record.hash
            isDirty = false
            saving = false
            lastError = nil
            // The write itself fires the watcher ~0.2 s later; the recorded
            // hash dedups it in vaultChanged.
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

    // MARK: - Watcher (one "vault changed" pass per FSEvents batch)

    /// The coalesced batch from `VaultWatcher` (main queue). Rebuild the
    /// tree; reload the open note only if its path is in the batch (or the
    /// root itself changed); self-writes dedup by the recorded hash.
    func vaultChanged(_ change: VaultWatcher.Change) {
        reloadTree()
        // Bridge contract: `setVaultIndex` is sent "on load and after every
        // vault change" — a broken link in the open note starts resolving
        // the moment its target appears, without reopening.
        editor.sendVaultIndex(paths: notePaths())
        refreshBacklinks()
        if searchActive { runSearch(lastSearchQuery) }

        guard let open = openPath else { return }
        let inBatch = change.rootChanged || change.paths.contains(open)
        guard inBatch else { return }

        // Self-write dedup: an FSEvent for our own atomic write whose
        // on-disk hash equals the hash we just wrote is not an external change.
        if let record = lastWrite, record.path == open,
           let onDisk = try? vault.read(path: open).value,
           onDisk.hash == record.hash {
            lastWrite = nil
            return
        }

        guard let onDisk = try? vault.read(path: open).value else {
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

    /// The active (trimmed) search query, re-run on vault change so live
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
