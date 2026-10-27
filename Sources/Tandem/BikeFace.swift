import SwiftUI
import TandemCore

// The Bikeshed surface (spec gui-structure; ticket 02's settled layout —
// in-tree search, bottom backlinks dock):
//
//   Sidebar 232 pt — "Bikeshed" + new-note, in-tree search field, the
//                    folder/note tree (search replaces it while typing)
//   Editor column  — 42 pt title bar (folder path + title + transient
//                    "saving…"), the persistent WKWebView (the bridge host),
//                    the bottom Backlinks dock
//
// All decisions live in BikeshedModel; this face only draws and forwards.

enum MainTab: String, CaseIterable, Identifiable {
    case tickets = "Tickets"
    case board = "Board"
    case inbox = "Inbox"
    /// The session records — the artifacts an agent session leaves behind
    /// (rows in the store's `sessions` table), read-only, and the place a
    /// session's record is read now that the sessions left the board (spec
    /// `FC770B8A`).
    case sessions = "Sessions"
    /// The sprint records — the artifacts a sprint leaves behind
    /// (rows in the store's `sprints` table), read-only, in the record's own
    /// two states (`open`, `closed`) — never the board's (spec `4F746EFE`, R3).
    case sprints = "Sprints"
    case bikeshed = "Bikeshed"
    var id: String { rawValue }
}

/// The main window's tab control (settled: the second tab in the main window,
/// header row after the wordmark).
struct BikeTabControl: View {
    @Binding var tab: MainTab
    var palette: Palette
    /// What is waiting in the inbox — drawn as a dot on the tab, so an agent's request for
    /// a decision is visible from the other faces.
    var inboxCount: Int = 0

    var body: some View {
        HStack(spacing: 0) {
            ForEach(MainTab.allCases) { t in
                Button {
                    withAnimation(Motion.toggle) { tab = t }
                } label: {
                    HStack(spacing: 4) {
                        Text(t.rawValue)
                            .font(.system(size: 12.5, weight: tab == t ? .semibold : .regular))
                            .foregroundStyle(tab == t ? palette.ink : palette.inkDim)
                        if t == .inbox, inboxCount > 0 {
                            Circle().fill(palette.ember).frame(width: 5, height: 5)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .frame(height: Space.control)
                    .background(tab == t ? AnyShapeStyle(palette.parchment)
                                         : AnyShapeStyle(Color.clear))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        // 504 pt: six labelled tabs at the 760 pt window floor — 84 pt per
        // tab, the per-tab width the fifth tab's move settled (340 → 420),
        // so every label ("Sessions" the widest at ~62 pt) draws whole with
        // no truncation.
        .frame(width: 504, height: Space.control)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.paper))
        .clipShape(RoundedRectangle(cornerRadius: Radius.control))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
    }
}

struct BikeFaceView: View {
    var model: BikeshedModel
    @Environment(\.colorScheme) private var colorScheme

    @State private var query = ""
    @State private var dockCollapsed = false
    @State private var renameTarget: RenameTarget?
    @State private var renameText = ""

    private var palette: Palette { Palette.forScheme(colorScheme) }

    var body: some View {
        VStack(spacing: 0) {
            if let error = model.lastError {
                LigneBanner(palette: palette,
                            systemImage: "xmark.circle.fill",
                            title: error,
                            onDismiss: { model.dismissError() })
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, Space.gutter)
                    .padding(.top, Space.lane)
            }
            // The friendly empty state is only for a bikeshed with *nothing* in it.
            // A bikeshed holding folders but no notes still shows the tree, so the
            // human can keep building the structure (and add notes inside it).
            if model.tree.isEmpty {
                emptyBikeshed
            } else {
                HStack(spacing: 0) {
                    BikeTreeView(model: model, palette: palette, query: $query,
                                 onRename: beginRename)
                        .frame(width: 232)
                    Rectangle().fill(palette.sand).frame(width: Line.hairline)
                    editorColumn
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(palette.paper)
        // Esc clears the search query (the field or its results have focus).
        .background {
            Button("") { clearQuery() }
                .keyboardShortcut(.escape, modifiers: [])
                .buttonStyle(.plain)
                .hidden()
        }
        // ⌘S saves now (bypasses the ~1 s autosave debounce).
        .background {
            Button("") { model.flushSave() }
                .keyboardShortcut("s", modifiers: .command)
                .buttonStyle(.plain)
                .hidden()
        }
        .sheet(item: $renameTarget) { target in
            renameSheet(target)
        }
    }

    // MARK: editor column

    private var editorColumn: some View {
        VStack(spacing: 0) {
            if let path = model.openPath {
                titleBar(path)
                Rectangle().fill(palette.sand).frame(height: Line.hairline)
                // The project's scaffold strip (4E5E9A0B): when the open note
                // lives in a project, its page set — Overview, Decisions,
                // Risks, Processes — is one click away from any of its pages.
                if let front = model.projectFront(for: path) {
                    BikeProjectStrip(model: model, front: front, openPath: path,
                                     palette: palette)
                    Rectangle().fill(palette.sand).frame(height: Line.hairline)
                }
                if model.conflict {
                    conflictBanner
                        .padding(.horizontal, 16)
                        .padding(.top, 10)
                }
                // The persistent WKWebView (owned by the model, so it survives
                // tab switches — no page reload, the document stays).
                BikeEditorPane(model: model)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                backlinksDock
            } else if !model.frontPage.isEmpty {
                // The wiki front page (4E5E9A0B): the tab's home — one glance,
                // every project's state.
                frontPageView
            } else {
                noNotePlaceholder
            }
        }
    }

    private func titleBar(_ path: String) -> some View {
        HStack(spacing: 8) {
            Text(BikeshedPath.folder(of: path))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(palette.inkDim)
            Text(BikeshedPath.name(of: path))
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(palette.ink)
                .lineLimit(1)
            Spacer(minLength: 8)
            // Autosave is debounced; no persistent unsaved badge — at most the
            // transient mark while a write is in flight.
            if model.saving {
                Text("saving…")
                    .font(.system(size: 11))
                    .foregroundStyle(palette.inkDim)
                    .transition(.opacity)
            }
            // "Send to tickets": the selection (or the caret's checkbox line)
            // becomes a ticket in the tickets list, linked to this note. The
            // note itself is not modified. The shortcut is the editor's own
            // (⌘⇧T), so it works where the selection is.
            TextButton(title: "Send to tickets", palette: palette) {
                model.requestSendToTickets()
            }
            .help("Send the selection to the tickets list (⌘⇧T)")
        }
        .padding(.horizontal, 16)
        .frame(height: 42)
    }

    /// The conflict banner: the buffer is dirty AND the note changed on disk.
    /// The two choices are the only way out (no dismiss); the pending
    /// autosave is withheld while it is up.
    private var conflictBanner: some View {
        LigneBanner(palette: palette,
                    systemImage: "exclamationmark.triangle",
                    title: "This note changed on disk while you were editing it.",
                    accentBar: true,
                    actions: [
                        ("Overwrite with mine", model.conflictChooseMine),
                        ("Take theirs", model.conflictChooseTheirs),
                    ],
                    showDismiss: false)
    }

    // MARK: backlinks — the bottom dock (the settled v1 style)

    private var backlinksDock: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                Image(systemName: "arrow.left.arrow.right")
                    .font(.system(size: 9.5, weight: .medium))
                    .foregroundStyle(palette.inkDim)
                Text("Backlinks")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(palette.inkDim)
                Text("\(model.backlinks.count)")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(palette.inkDim)
                    .monospacedDigit()
                Spacer()
                Button {
                    withAnimation(Motion.toggle) { dockCollapsed.toggle() }
                } label: {
                    Image(systemName: dockCollapsed ? "chevron.up" : "chevron.down")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(palette.inkDim)
                        .frame(width: 20, height: 20)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 16)
            .frame(height: 30)
            if !dockCollapsed {
                Rectangle().fill(palette.sand).frame(height: Line.hairline)
                Group {
                    if model.backlinks.isEmpty {
                        Text("Nothing links here yet.")
                            .font(.system(size: 11.5))
                            .foregroundStyle(palette.inkDim)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 8)
                    } else {
                        ScrollView {
                            LazyVStack(spacing: 0) {
                                ForEach(model.backlinks, id: \.path) { link in
                                    backlinkRow(link)
                                    if link.path != model.backlinks.last?.path {
                                        Rectangle().fill(palette.sand)
                                            .frame(height: Line.hairline)
                                            .padding(.leading, 34)
                                    }
                                }
                            }
                        }
                        .frame(maxHeight: 132)
                    }
                }
            }
        }
    }

    /// One backlink: the linking note (clickable) + the line that mentions the
    /// open note (core `backlinks`: one row per linking note).
    private func backlinkRow(_ link: Backlink) -> some View {
        Button {
            model.open(link.path)
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Image(systemName: "doc.text")
                        .font(.system(size: 10))
                        .foregroundStyle(palette.accent)
                    Text(BikeshedPath.name(of: link.path))
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(palette.accent)
                        .lineLimit(1)
                }
                Text(link.line)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(palette.inkDim)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 14)
            .padding(.vertical, 7)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: empty states (the designed ones, not errors)

    /// Empty bikeshed (the app creates the folder lazily, seeds no welcome note).
    private var emptyBikeshed: some View {
        VStack(spacing: 18) {
            BikeScene(palette: palette, riders: false, ground: true)
                .frame(width: 190,
                       height: 190 * BikeArt.sceneSize.height / BikeArt.sceneSize.width)
            VStack(spacing: 4) {
                Text("The bikeshed is empty")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(palette.ink)
                Text("Notes are plain markdown files in \(model.bikeshed.directory.path) —\nadd one here, or let an agent write one.")
                    .font(.system(size: 12))
                    .foregroundStyle(palette.inkDim)
                    .multilineTextAlignment(.center)
            }
            HStack(spacing: 8) {
                newNoteButton
                newFolderButton
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var noNotePlaceholder: some View {
        VStack(spacing: 14) {
            BikeScene(palette: palette, riders: false)
                .frame(width: 100,
                       height: 100 * BikeArt.sceneSize.height / BikeArt.sceneSize.width)
                .opacity(0.85)
            VStack(spacing: 3) {
                Text("No note selected")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(palette.ink)
                Text("Pick a note in the tree, or create one.")
                    .font(.system(size: 12))
                    .foregroundStyle(palette.inkDim)
            }
            newNoteButton
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: front page (the tab's home — one glance, every project's state)

    /// The wiki front page (4E5E9A0B): one row per project — the project's
    /// name (clickable → its Overview), the freshness stamp (the Overview's
    /// on-disk mtime — the consolidation pass rewrites the Overview exactly
    /// when the state changes), the "where this left off" line, and the
    /// project's scaffold page set as chips. Sub-pages are the wikilinks in
    /// the Overview — the editor renders those clickable already.
    private var frontPageView: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline) {
                Text("Bikeshed")
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(palette.ink)
                Text("every project, where it left off")
                    .font(.system(size: 12))
                    .foregroundStyle(palette.inkDim)
            }
            .padding(.bottom, 12)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(model.frontPage, id: \.folder) { row in
                        projectRow(row)
                        if row.folder != model.frontPage.last?.folder {
                            Rectangle().fill(palette.sand)
                                .frame(height: Line.hairline)
                        }
                    }
                }
            }
        }
        .padding(.horizontal, 24)
        .padding(.top, 18)
        .padding(.bottom, 12)
        .frame(maxWidth: 640)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }

    /// One project row: name + freshness stamp, the state line, the scaffold
    /// chips. The name opens the Overview.
    private func projectRow(_ row: ProjectFront) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Button {
                    if let overview = row.pages.first(where: { $0.role == .overview }) {
                        model.open(overview.path)
                    }
                } label: {
                    Text(row.name)
                        .font(.system(size: 13.5, weight: .semibold))
                        .foregroundStyle(palette.accent)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Open \(row.name)'s Overview")
                Spacer()
                if let fresh = row.freshness {
                    Text("updated \(fresh.formatted(.dateTime.month().day()))")
                        .font(.system(size: 11))
                        .foregroundStyle(palette.inkDim)
                        .monospacedDigit()
                }
            }
            if let state = row.stateLine {
                Text(state)
                    .font(.system(size: 12))
                    .foregroundStyle(palette.ink)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 6) {
                ForEach(row.pages, id: \.path) { page in
                    BikePageChip(title: page.role.label, palette: palette,
                                 selected: false) {
                        model.open(page.path)
                    }
                }
            }
        }
        .padding(.vertical, 12)
    }

    private var newNoteButton: some View {
        Button {
            model.createNote(in: nil)
        } label: {
            Text("New note")
                .font(.system(size: 12.5, weight: .semibold))
                .foregroundStyle(palette.paper)
                .padding(.horizontal, 14)
                .frame(height: 30)
                .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.accent))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    /// The empty state's secondary action: start the project structure with a
    /// root folder (the tree then appears, so folders can nest). Quiet, so the
    /// primary "New note" stays the obvious first move.
    private var newFolderButton: some View {
        Button {
            model.createFolder(in: nil)
        } label: {
            Text("New folder")
                .font(.system(size: 12.5, weight: .medium))
                .foregroundStyle(palette.inkDim)
                .padding(.horizontal, 14)
                .frame(height: 30)
                .background(RoundedRectangle(cornerRadius: Radius.control)
                    .strokeBorder(palette.line, lineWidth: Line.hairline))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Create a folder to organise notes into projects")
    }

    // MARK: rename (the tree's context menu)

    private struct RenameTarget: Identifiable {
        let path: String
        var id: String { path }
    }

    private func beginRename(_ path: String) {
        renameText = path
        renameTarget = RenameTarget(path: path)
    }

    private func renameSheet(_ target: RenameTarget) -> some View {
        VStack(spacing: 14) {
            Text("Rename note")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(palette.ink)
            Text("The bikeshed-root-relative path (must end in .md).")
                .font(.system(size: 11.5))
                .foregroundStyle(palette.inkDim)
            TextField("Path", text: $renameText)
                .textFieldStyle(.plain)
                .font(.system(size: 12.5, design: .monospaced))
                .foregroundStyle(palette.ink)
                .padding(.horizontal, 11)
                .frame(height: 28)
                .frame(maxWidth: .infinity)
                .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
                .overlay(RoundedRectangle(cornerRadius: Radius.control)
                    .strokeBorder(palette.line, lineWidth: Line.hairline))
            HStack {
                Button("Cancel") { renameTarget = nil }
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button("Rename") {
                    let to = renameText.trimmingCharacters(in: .whitespacesAndNewlines)
                    guard !to.isEmpty else { return }
                    model.renameNote(from: target.path, to: to)
                    renameTarget = nil
                }
                .keyboardShortcut(.defaultAction)
                .disabled(renameText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 420)
    }

    private func clearQuery() {
        query = ""
        model.setQuery("")
    }
}

// MARK: - sidebar (232 pt): header · in-tree search · tree / results

/// A tooltip that is only there when there is something to say — the tree's
/// hidden-layer rows carry one, the ordinary rows carry none.
private struct OptionalHelp: ViewModifier {
    var text: String?

    func body(content: Content) -> some View {
        if let text, !text.isEmpty {
            content.help(text)
        } else {
            content
        }
    }
}

/// The tooltip on a revealed hidden entry: what it is, and what it is not.
/// File-scoped so the tree rows (which are their own views) can carry it.
private let hiddenLayerHelp = "Hidden layer — outside search, backlinks, and wikilinks."

struct BikeTreeView: View {
    var model: BikeshedModel
    var palette: Palette
    @Binding var query: String
    var onRename: (String) -> Void
    /// True while a drag hovers the tree's background — the bikeshed root,
    /// the one destination that is not a folder row.
    @State private var rootTargeted = false

    var body: some View {
        VStack(spacing: 0) {
            // Sidebar header: label + today + new-note. The label is the way
            // home: clicking it closes the note and shows the front page.
            HStack(spacing: 6) {
                Button {
                    model.closeNote(silent: false)
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "folder")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(palette.inkDim)
                        Text("Bikeshed")
                            .font(.system(size: 12.5, weight: .semibold))
                            .foregroundStyle(palette.ink)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Back to the front page")
                Spacer()
                // Today's note, one click: the daily note is the note the
                // human keeps, and it lives behind a menu dig otherwise.
                Button {
                    model.openToday()
                } label: {
                    Image(systemName: "calendar")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(palette.inkDim)
                        .frame(width: 20, height: 20)
                        .background(RoundedRectangle(cornerRadius: Radius.checkbox)
                            .strokeBorder(palette.line, lineWidth: Line.hairline))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Today's note")
                Menu {
                    Button("New note") { model.createNote(in: nil) }
                    Button("New folder") { model.createFolder(in: nil) }
                    Button("Today's note") { model.openToday() }
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(palette.inkDim)
                        .frame(width: 20, height: 20)
                        .background(RoundedRectangle(cornerRadius: Radius.checkbox)
                            .strokeBorder(palette.line, lineWidth: Line.hairline))
                        .contentShape(Rectangle())
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .help("New note or folder")
            }
            .padding(.horizontal, 12)
            .padding(.top, 10)
            .padding(.bottom, 8)

            Rectangle().fill(palette.sand).frame(height: Line.hairline)

            // In-tree search: the field lives at the top of the sidebar;
            // typing swaps the tree for bikeshed-wide results (debounced in the
            // model; the scan runs off-main).
            HStack(spacing: 7) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 11))
                    .foregroundStyle(palette.inkDim)
                TextField("Search notes…", text: $query)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12.5))
                    .foregroundStyle(palette.ink)
                    .onChange(of: query) { _, newValue in
                        model.setQuery(newValue)
                    }
            }
            .padding(.horizontal, 11)
            .frame(height: 28)
            .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
            .overlay(RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(palette.line, lineWidth: Line.hairline))
            .padding(.horizontal, 10)
            .padding(.top, 8)

            Group {
                if model.searchActive {
                    searchResults
                } else {
                    tree
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            // The Health panel (the link-graph report) sits under the tree:
            // the broken wikilinks and the orphan notes, clickable through
            // to the note that holds them.
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            BikeHealthPanel(model: model, palette: palette)
        }
    }

    // MARK: tree

    private var tree: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(model.treeChildren(of: ""), id: \.path) { item in
                    node(item, depth: 0)
                }
            }
            .padding(.vertical, 6)
        }
        // The tree's background is the bikeshed root: a drop that lands on no
        // folder moves the dragged note or folder to the root.
        .dropDestination(for: String.self) { items, _ in
            guard let path = items.first else { return false }
            model.move(path, into: "")
            return true
        } isTargeted: { rootTargeted = $0 }
        .background(rootTargeted ? AnyShapeStyle(palette.parchment) : AnyShapeStyle(Color.clear))
    }

    /// AnyView: the tree recurses, and `some View` can't name itself.
    private func node(_ item: BikeshedNode, depth: Int) -> AnyView {
        switch item.kind {
        case .folder:
            return AnyView(
                Group {
                    BikeFolderRow(model: model, palette: palette, item: item, depth: depth)
                    if model.expandedFolders.contains(item.path) {
                        ForEach(model.treeChildren(of: item.path), id: \.path) { child in
                            node(child, depth: depth + 1)
                        }
                    }
                }
            )
        case .note:
            return AnyView(
                BikeNoteRow(model: model, palette: palette, item: item, depth: depth,
                            onRename: onRename)
            )
        }
    }

    // MARK: search results (replace the tree while a query is active)

    private var searchResults: some View {
        let results = model.searchResults
        return Group {
            if results.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("No matches")
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(palette.ink)
                    Text("Nothing in the bikeshed matches “\(query)”.")
                        .font(.system(size: 11.5))
                        .foregroundStyle(palette.inkDim)
                        .lineLimit(2)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(results, id: \.path) { hit in
                            Button {
                                model.open(hit.path)
                                query = ""
                                model.setQuery("")
                            } label: {
                                VStack(alignment: .leading, spacing: 2) {
                                    HStack(spacing: 5) {
                                        Image(systemName: "doc.text")
                                            .font(.system(size: 10))
                                            .foregroundStyle(palette.inkDim)
                                        Text(BikeshedPath.name(of: hit.path))
                                            .font(.system(size: 12.5, weight: .medium))
                                            .foregroundStyle(palette.ink)
                                            .lineLimit(1)
                                    }
                                    Text(hit.display)
                                        .font(.system(size: 11, design: .monospaced))
                                        .foregroundStyle(palette.inkDim)
                                        .lineLimit(1)
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, 12)
                                .padding(.vertical, 5)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            if hit.path != results.last?.path {
                                Rectangle().fill(palette.sand)
                                    .frame(height: Line.hairline)
                                    .padding(.leading, 24)
                            }
                        }
                    }
                    .padding(.vertical, 4)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}

// MARK: - Tree rows (drag sources; the folder row is a drop target)

/// A folder row in the tree: the disclosure chevron, the folder icon, the
/// name. It is the tree's drop target — dropping a note or a folder here
/// moves it into this folder. The core does the work: it rewrites every
/// wikilink that resolves to a moved note (a folder move: every note inside
/// it) and moves the file in one locked transaction; a cycle (a folder into
/// its own descendant) and a name already taken in the destination are
/// refused, with the reason shown.
private struct BikeFolderRow: View {
    var model: BikeshedModel
    var palette: Palette
    var item: BikeshedNode
    var depth: Int
    /// True while a drag hovers this row — the drop lands here.
    @State private var targeted = false

    var body: some View {
        // A revealed hidden entry (the capture saddlebag, the consolidation
        // archive) is drawn dimmed: it is out of the curated graph, and the
        // human should see that at a glance.
        let hidden = Bikeshed.isHiddenPath(item.path)
        return Button {
            model.toggleFolder(item.path)
        } label: {
            HStack(spacing: 5) {
                Image(systemName: model.expandedFolders.contains(item.path)
                      ? "chevron.down" : "chevron.right")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(palette.inkDim)
                    .frame(width: 9)
                Image(systemName: "folder")
                    .font(.system(size: 10.5))
                    .foregroundStyle(palette.inkDim)
                Text(item.name)
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(hidden ? palette.inkDim : palette.ink)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.leading, 10 + CGFloat(depth) * 14)
        .contextMenu {
            Button("New note") { model.createNote(in: item.path) }
            Button("New folder") { model.createFolder(in: item.path) }
            Button("Delete", role: .destructive) { model.delete(item.path) }
        }
        .draggable(item.path) { BikeDragPreview(name: item.name, palette: palette) }
        .dropDestination(for: String.self) { items, _ in
            guard let path = items.first else { return false }
            model.move(path, into: item.path)
            return true
        } isTargeted: { targeted = $0 }
        .background(targeted ? AnyShapeStyle(palette.parchment) : AnyShapeStyle(Color.clear))
        .overlay(alignment: .leading) {
            if targeted {
                Rectangle().fill(palette.accent).frame(width: 2)
            }
        }
        // The drop hint and the hidden-layer note share one tooltip.
        .help(hidden
              ? hiddenLayerHelp + " Drop a note or folder here to move it into \"\(item.name)\""
              : "Drop a note or folder here to move it into \"\(item.name)\"")
    }
}

/// A note row in the tree: the note icon and the name, highlighted while the
/// note is open. It is a drag source — dragging a note onto a folder moves
/// it there (the core rewrites the links to it); dropping it on the tree's
/// background moves it to the bikeshed root.
private struct BikeNoteRow: View {
    var model: BikeshedModel
    var palette: Palette
    var item: BikeshedNode
    var depth: Int
    var onRename: (String) -> Void

    var body: some View {
        let selected = model.openPath == item.path
        // A revealed hidden entry (the capture saddlebag, the consolidation
        // archive) is drawn dimmed: it is out of the curated graph, and the
        // human should see that at a glance.
        let hidden = Bikeshed.isHiddenPath(item.path)
        return Button {
            model.open(item.path)
        } label: {
            HStack(spacing: 5) {
                Image(systemName: "doc.text")
                    .font(.system(size: 10.5))
                    .foregroundStyle(selected ? palette.paper.opacity(0.8)
                                              : palette.inkDim)
                Text(item.name)
                    .font(.system(size: 12.5))
                    .foregroundStyle(selected ? palette.paper
                              : (hidden ? palette.inkDim : palette.ink))
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
            .background(selected ? AnyShapeStyle(palette.accent)
                                 : AnyShapeStyle(Color.clear))
        }
        .buttonStyle(.plain)
        .padding(.leading, 10 + CGFloat(depth) * 14 + 14)
        .modifier(OptionalHelp(text: hidden ? hiddenLayerHelp : nil))
        .contextMenu {
            // Renaming a note is also how it moves to another folder (the
            // path is the name); the drag is the way to move a folder.
            Button("Rename…") { onRename(item.path) }
            Button("Delete", role: .destructive) { model.delete(item.path) }
        }
        .draggable(item.path) { BikeDragPreview(name: item.name, palette: palette) }
    }
}

/// The drag image for a tree row: the item's name on a small parchment card,
/// so the drag reads as "this note", not a raw path.
private struct BikeDragPreview: View {
    var name: String
    var palette: Palette

    var body: some View {
        Text(name)
            .font(.system(size: 12.5))
            .foregroundStyle(palette.ink)
            .lineLimit(1)
            .frame(maxWidth: 220, alignment: .leading)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
            .overlay(RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(palette.line, lineWidth: Line.hairline))
    }
}

// MARK: - Scaffold chips (the project's page set)

/// One scaffold-page chip: the role's label, clickable → the page. Selected
/// = the note open in the editor (the strip's state).
private struct BikePageChip: View {
    var title: String
    var palette: Palette
    var selected: Bool
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 11, weight: selected ? .semibold : .medium))
                .foregroundStyle(selected ? palette.paper : palette.ink)
                .padding(.horizontal, 9)
                .frame(height: 22)
                .background(selected ? AnyShapeStyle(palette.accent)
                                     : AnyShapeStyle(Color.clear))
                .overlay(RoundedRectangle(cornerRadius: Radius.control)
                    .strokeBorder(selected ? palette.accent : palette.line,
                                  lineWidth: Line.hairline))
                .clipShape(RoundedRectangle(cornerRadius: Radius.control))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// The editor column's scaffold strip (4E5E9A0B): when the open note lives
/// in a project, the project's page set — Overview, Decisions, Risks,
/// Processes — draws as chips under the title bar, the open page highlighted.
/// The chips are the same derivation the front page's rows carry
/// (`Bikeshed.frontPage()`), so the strip and the home can never disagree.
private struct BikeProjectStrip: View {
    var model: BikeshedModel
    var front: ProjectFront
    var openPath: String
    var palette: Palette

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                Image(systemName: "square.stack")
                    .font(.system(size: 10))
                    .foregroundStyle(palette.inkDim)
                ForEach(front.pages, id: \.path) { page in
                    BikePageChip(title: page.role.label, palette: palette,
                                 selected: page.path == openPath) {
                        model.open(page.path)
                    }
                }
            }
        }
        .padding(.horizontal, 16)
        .frame(height: 30)
    }
}

// MARK: - Health panel (the link-graph report)

// The Health panel: what the bikeshed's links look like — the wikilinks that
// resolve to nothing, and the notes nothing links to. It reads the same core
// call the CLI's `note links` makes (`Bikeshed.health()`), so the panel and
// the command always report the same set.
//
// A row is clickable: it opens the note that holds the broken link (the place
// the fix goes) or the orphan note itself.

struct BikeHealthPanel: View {
    var model: BikeshedModel
    var palette: Palette
    @State private var collapsed = false

    private var health: BikeshedHealth { model.health }
    private var issueCount: Int { health.counts.broken + health.counts.orphans }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                Image(systemName: issueCount == 0 ? "checkmark.seal" : "exclamationmark.triangle")
                    .font(.system(size: 9.5, weight: .medium))
                    .foregroundStyle(issueCount == 0 ? palette.inkDim : palette.ember)
                Text("Health")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(palette.inkDim)
                Text(summary)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(palette.inkDim)
                    .monospacedDigit()
                Spacer()
                Button {
                    withAnimation(Motion.toggle) { collapsed.toggle() }
                } label: {
                    Image(systemName: collapsed ? "chevron.up" : "chevron.down")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(palette.inkDim)
                        .frame(width: 20, height: 20)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("Broken links and orphan notes")
            }
            .padding(.horizontal, 12)
            .frame(height: 28)

            if !collapsed {
                Rectangle().fill(palette.sand).frame(height: Line.hairline)
                rows
            }
        }
    }

    /// The header's count: what is wrong, or "all clear".
    private var summary: String {
        if issueCount == 0 { return "all clear" }
        var parts: [String] = []
        if health.counts.broken > 0 { parts.append("\(health.counts.broken) broken") }
        if health.counts.orphans > 0 { parts.append("\(health.counts.orphans) orphan") }
        return parts.joined(separator: " · ")
    }

    private var rows: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if issueCount == 0 {
                    Text("No broken links, no orphan notes.")
                        .font(.system(size: 11.5))
                        .foregroundStyle(palette.inkDim)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                }
                ForEach(health.broken, id: \.self) { link in
                    brokenRow(link)
                }
                ForEach(health.orphans, id: \.self) { path in
                    orphanRow(path)
                }
            }
        }
        .frame(maxHeight: 132)
    }

    /// A broken link: the note that holds it, the target that resolves to
    /// nothing, and the line it sits on.
    private func brokenRow(_ link: BrokenLink) -> some View {
        Button {
            model.open(link.note)
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "link.badge.plus")
                    .font(.system(size: 10))
                    .foregroundStyle(palette.ember)
                VStack(alignment: .leading, spacing: 1) {
                    Text(BikeshedPath.name(of: link.note))
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(palette.ink)
                        .lineLimit(1)
                    Text("→ \(link.link) · line \(link.line)")
                        .font(.system(size: 10.5, design: .monospaced))
                        .foregroundStyle(palette.inkDim)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.vertical, 5)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Open \(link.note)")
    }

    /// An orphan note: nothing links to it.
    private func orphanRow(_ path: String) -> some View {
        Button {
            model.open(path)
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "doc.text")
                    .font(.system(size: 10))
                    .foregroundStyle(palette.inkDim)
                VStack(alignment: .leading, spacing: 1) {
                    Text(BikeshedPath.name(of: path))
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(palette.ink)
                        .lineLimit(1)
                    Text("nothing links here")
                        .font(.system(size: 10.5, design: .monospaced))
                        .foregroundStyle(palette.inkDim)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.vertical, 5)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Open \(path)")
    }
}
