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
    case todos = "Todos"
    case board = "Board"
    case inbox = "Inbox"
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
        .frame(width: 340, height: Space.control)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.paper))
        .clipShape(RoundedRectangle(cornerRadius: Radius.control))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
    }
}

/// The display title of a note path: the last component, minus the `.md`.
fileprivate func noteTitle(_ path: String) -> String {
    var name = path.split(separator: "/").last.map(String.init) ?? path
    if name.hasSuffix(".md") { name = String(name.dropLast(3)) }
    return name
}

/// The note's folder (the path minus its last component), or "root".
fileprivate func noteFolder(_ path: String) -> String {
    if let slash = path.lastIndex(of: "/") { return String(path[..<slash]) }
    return "root"
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
            } else {
                noNotePlaceholder
            }
        }
    }

    private func titleBar(_ path: String) -> some View {
        HStack(spacing: 8) {
            Text(noteFolder(path))
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(palette.inkDim)
            Text(noteTitle(path))
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
            // "Send to todos": the selection (or the caret's checkbox line)
            // becomes a ticket in the todos list, linked to this note. The
            // note itself is not modified. The shortcut is the editor's own
            // (⌘⇧T), so it works where the selection is.
            TextButton(title: "Send to todos", palette: palette) {
                model.requestSendToTodos()
            }
            .help("Send the selection to the todos list (⌘⇧T)")
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
                    Text(noteTitle(link.path))
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

struct BikeTreeView: View {
    var model: BikeshedModel
    var palette: Palette
    @Binding var query: String
    var onRename: (String) -> Void

    var body: some View {
        VStack(spacing: 0) {
            // Sidebar header: label + new-note
            HStack(spacing: 6) {
                Image(systemName: "folder")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(palette.inkDim)
                Text("Bikeshed")
                    .font(.system(size: 12.5, weight: .semibold))
                    .foregroundStyle(palette.ink)
                Spacer()
                Menu {
                    Button("New note") { model.createNote(in: nil) }
                    Button("New folder") { model.createFolder(in: nil) }
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
    }

    /// AnyView: the tree recurses, and `some View` can't name itself.
    private func node(_ item: BikeshedNode, depth: Int) -> AnyView {
        switch item.kind {
        case .folder:
            return AnyView(
                Group {
                    Button {
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
                                .foregroundStyle(palette.ink)
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
                    if model.expandedFolders.contains(item.path) {
                        ForEach(model.treeChildren(of: item.path), id: \.path) { child in
                            node(child, depth: depth + 1)
                        }
                    }
                }
            )
        case .note:
            let selected = model.openPath == item.path
            return AnyView(
                Button {
                    model.open(item.path)
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: "doc.text")
                            .font(.system(size: 10.5))
                            .foregroundStyle(selected ? palette.paper.opacity(0.8)
                                                      : palette.inkDim)
                        Text(item.name)
                            .font(.system(size: 12.5))
                            .foregroundStyle(selected ? palette.paper : palette.ink)
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
                .contextMenu {
                    // Moving a note is the Rename… path change; folder rename
                    // is out of scope (the core's rename stays notes-only).
                    Button("Rename…") { onRename(item.path) }
                    Button("Delete", role: .destructive) { model.delete(item.path) }
                }
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
                                        Text(noteTitle(hit.path))
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
