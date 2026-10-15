import SwiftUI
import Observation

// PROTOTYPE — the Bikeshed surface (bikeshed map, ticket 02). Three
// structurally different layouts, composed from the same shared parts:
//
//   search style  (model.searchStyle)
//     .tree   — field at the top of the tree sidebar; results replace the tree
//     .bar    — field in the editor's title bar; results drop from the bar
//     .panel  — a dedicated right panel opened from the title bar
//   backlink style (model.backlinkStyle)
//     .bottom — a "Linked mentions" dock under the editor
//     .side   — a right rail beside the editor
//   (.panel + .side share the right rail: the search panel takes it while
//   open, backlinks own it while the panel is closed.)
//
// The editor is the live-preview mock from Markdown.swift — ticket 01
// settled the real tech (CM6 in a WKWebView); this face is judged for
// surface, not editing.

enum MainTab: String, CaseIterable, Identifiable {
    case todos = "Todos"
    case bikeshed = "Bikeshed"
    var id: String { rawValue }
}

/// The main-window tab control (map, settled): second tab in the main window.
struct BikeTabControl: View {
    @Binding var tab: MainTab
    var pal: Palette

    var body: some View {
        HStack(spacing: 0) {
            ForEach(MainTab.allCases) { t in
                Button {
                    withAnimation(Motion.toggle) { tab = t }
                } label: {
                    Text(t.rawValue)
                        .font(.system(size: 12.5, weight: tab == t ? .semibold : .regular))
                        .foregroundStyle(tab == t ? pal.paper : pal.ink)
                        .frame(maxWidth: .infinity)
                        .frame(height: 28)
                        .background(tab == t ? AnyShapeStyle(pal.dustyBlue)
                                             : AnyShapeStyle(Color.clear))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .frame(width: 190, height: 28)
        .background(RoundedRectangle(cornerRadius: Radius.control).fill(pal.paper))
        .clipShape(RoundedRectangle(cornerRadius: Radius.control))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(pal.ink, lineWidth: Line.hairline))
    }
}

struct BikeshedFaceView: View {
    @Bindable var model: BikeModel
    var pal: Palette

    @State private var dockCollapsed = false
    @State private var panelOpen = false

    var body: some View {
        Group {
            if model.notes.isEmpty {
                emptyVault
            } else {
                HStack(spacing: 0) {
                    BikeTreeView(model: model, pal: pal, searchInTree: model.searchStyle == .tree)
                        .frame(width: 232)
                    Rectangle().fill(pal.sand).frame(width: Line.hairline)
                    editorColumn
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                    if showRightRail {
                        Rectangle().fill(pal.sand).frame(width: Line.hairline)
                        rightRail
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(pal.paper)
        // Esc clears the search query (bar dropdown / panel / tree field).
        .background {
            Button("") { model.query = "" }
                .keyboardShortcut(.escape, modifiers: [])
                .buttonStyle(.plain)
                .hidden()
        }
    }

    // MARK: layout

    private var showRightRail: Bool {
        if model.searchStyle == .panel { return panelOpen || model.backlinkStyle == .side }
        return model.backlinkStyle == .side
    }

    @ViewBuilder
    private var rightRail: some View {
        if model.searchStyle == .panel, panelOpen {
            searchPanel.frame(width: 264)
        } else if model.backlinkStyle == .side {
            backlinksRail.frame(width: 224)
        }
    }

    private var editorColumn: some View {
        VStack(spacing: 0) {
            if let note = model.openNote {
                titleBar(note)
                Rectangle().fill(pal.sand).frame(height: Line.hairline)
                previewArea(note)
                if model.backlinkStyle == .bottom {
                    backlinksDock
                }
            } else {
                noNotePlaceholder
            }
        }
    }

    // MARK: title bar

    private func titleBar(_ note: BikeNote) -> some View {
        HStack(spacing: 8) {
            Text(note.folder.isEmpty ? "root" : note.folder)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(pal.inkDim)
            Text(note.title)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(pal.ink)
                .lineLimit(1)
            Spacer(minLength: 8)

            switch model.searchStyle {
            case .tree:
                EmptyView()
            case .bar:
                HStack(spacing: 7) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 11))
                        .foregroundStyle(pal.inkDim)
                    TextField("Search notes…", text: $model.query)
                        .textFieldStyle(.plain)
                        .font(.system(size: 12.5))
                        .foregroundStyle(pal.ink)
                }
                .padding(.horizontal, 11)
                .frame(width: 210, height: 28)
                .background(RoundedRectangle(cornerRadius: Radius.control).fill(pal.parchment))
                .overlay(RoundedRectangle(cornerRadius: Radius.control)
                    .strokeBorder(pal.ink, lineWidth: Line.hairline))
            case .panel:
                Button {
                    panelOpen.toggle()
                    if !panelOpen { model.query = "" }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: "magnifyingglass")
                            .font(.system(size: 11))
                        Text("Search")
                            .font(.system(size: 12.5))
                    }
                    .foregroundStyle(panelOpen ? pal.paper : pal.ink)
                    .padding(.horizontal, 10)
                    .frame(height: 28)
                    .background(RoundedRectangle(cornerRadius: Radius.control)
                        .fill(panelOpen ? pal.dustyBlue : pal.parchment))
                    .overlay(RoundedRectangle(cornerRadius: Radius.control)
                        .strokeBorder(pal.ink, lineWidth: Line.hairline))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }

            // Autosave is debounced (ticket 04); no persistent unsaved badge —
            // at most the transient "saved" mark. Static in the prototype.
            Text("saved")
                .font(.system(size: 11))
                .foregroundStyle(pal.inkDim)
                .padding(.trailing, 2)
        }
        .padding(.horizontal, 16)
        .frame(height: 42)
    }

    // MARK: preview (+ bar-style results dropdown)

    private func previewArea(_ note: BikeNote) -> some View {
        ZStack(alignment: .top) {
            MarkdownView(note: note, model: model, pal: pal)
            if model.searchStyle == .bar, model.searchActive {
                // Click-catcher: a plain tap on the preview dismisses the
                // dropdown (wikilinks are unreachable while it is open).
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture { model.query = "" }
                BikeSearchResults(model: model, pal: pal, compact: false)
                    .frame(maxWidth: 480)
                    .frame(maxHeight: 320)
                    .background(RoundedRectangle(cornerRadius: Radius.control).fill(pal.parchment))
                    .overlay(RoundedRectangle(cornerRadius: Radius.control)
                        .strokeBorder(pal.ink, lineWidth: Line.hairline))
                    .padding(.horizontal, 16)
                    .padding(.top, 4)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // MARK: backlinks — bottom dock

    private var backlinks: [BikeNote] {
        guard let open = model.openPath else { return [] }
        return model.backlinks(for: open)
    }

    private var backlinksDock: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                Image(systemName: "arrow.left.arrow.right")
                    .font(.system(size: 9.5, weight: .medium))
                    .foregroundStyle(pal.inkDim)
                Text("Linked mentions")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(pal.inkDim)
                Text("\(backlinks.count)")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(pal.inkDim)
                    .monospacedDigit()
                Spacer()
                Button {
                    withAnimation(Motion.toggle) { dockCollapsed.toggle() }
                } label: {
                    Image(systemName: dockCollapsed ? "chevron.up" : "chevron.down")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(pal.inkDim)
                        .frame(width: 20, height: 20)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 16)
            .frame(height: 30)
            if !dockCollapsed {
                Rectangle().fill(pal.sand).frame(height: Line.hairline)
                Group {
                    if backlinks.isEmpty {
                        Text("Nothing links here yet.")
                            .font(.system(size: 11.5))
                            .foregroundStyle(pal.inkDim)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 8)
                    } else {
                        ScrollView {
                            LazyVStack(spacing: 0) {
                                ForEach(backlinks) { note in
                                    backlinkRow(note, maxHeight: 44)
                                    if note.id != backlinks.last?.id {
                                        Rectangle().fill(pal.sand).frame(height: Line.hairline)
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

    // MARK: backlinks — side rail

    private var backlinksRail: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                Image(systemName: "arrow.left.arrow.right")
                    .font(.system(size: 9.5, weight: .medium))
                    .foregroundStyle(pal.inkDim)
                Text("Backlinks")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(pal.inkDim)
                Text("\(backlinks.count)")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(pal.inkDim)
                    .monospacedDigit()
            }
            .padding(.horizontal, 14)
            .frame(height: 34)
            Rectangle().fill(pal.sand).frame(height: Line.hairline)
            Group {
                if backlinks.isEmpty {
                    Text("Nothing links to this note yet.")
                        .font(.system(size: 11.5))
                        .foregroundStyle(pal.inkDim)
                        .padding(14)
                } else {
                    ScrollView {
                        LazyVStack(spacing: 0) {
                            ForEach(backlinks) { note in
                                backlinkRow(note, maxHeight: 64)
                                if note.id != backlinks.last?.id {
                                    Rectangle().fill(pal.sand).frame(height: Line.hairline)
                                        .padding(.leading, 34)
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

    /// One backlink entry: the linking note (clickable) + the line that
    /// mentions it (ticket 03: backlinks = notes containing a wikilink that
    /// resolves to the open note; broken links contribute nothing).
    private func backlinkRow(_ note: BikeNote, maxHeight: CGFloat) -> some View {
        let context = model.openPath.flatMap {
            model.backlinkContext(from: note, to: $0)
        }
        return Button {
            model.open(note.path)
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Image(systemName: "doc.text")
                        .font(.system(size: 10))
                        .foregroundStyle(pal.dustyBlue)
                    Text(note.title)
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(pal.dustyBlue)
                        .lineLimit(1)
                }
                if let context {
                    Text(context)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(pal.inkDim)
                        .lineLimit(2)
                        .lineLimit(maxHeight > 50 ? 2 : 1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 14)
            .padding(.vertical, 7)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: search — dedicated panel

    private var searchPanel: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 10.5, weight: .medium))
                    .foregroundStyle(pal.inkDim)
                Text("Search")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(pal.inkDim)
                Text("\(model.results.count)")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(pal.inkDim)
                    .monospacedDigit()
                Spacer()
                Button {
                    panelOpen = false
                    model.query = ""
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(pal.inkDim)
                        .frame(width: 20, height: 20)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 14)
            .frame(height: 34)
            Rectangle().fill(pal.sand).frame(height: Line.hairline)
            HStack(spacing: 7) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 11))
                    .foregroundStyle(pal.inkDim)
                TextField("Search notes…", text: $model.query)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12.5))
                    .foregroundStyle(pal.ink)
            }
            .padding(.horizontal, 11)
            .frame(height: 28)
            .background(RoundedRectangle(cornerRadius: Radius.control).fill(pal.parchment))
            .overlay(RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(pal.ink, lineWidth: Line.hairline))
            .padding(10)
            BikeSearchResults(model: model, pal: pal, compact: false)
        }
    }

    // MARK: empty states

    /// Empty vault (ticket 04: the app creates the folder lazily, seeds no
    /// welcome note) — a designed state, not an error.
    private var emptyVault: some View {
        VStack(spacing: 18) {
            BikeScene(palette: pal, riders: false, ground: true)
                .frame(width: 190, height: 190 * BikeArt.sceneSize.height / BikeArt.sceneSize.width)
            VStack(spacing: 4) {
                Text("The bikeshed is empty")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(pal.ink)
                Text("Notes are plain markdown files in ~/.tandem/bikeshed/ —\nadd one here, or let an agent write one.")
                    .font(.system(size: 12))
                    .foregroundStyle(pal.inkDim)
                    .multilineTextAlignment(.center)
            }
            newNoteButton
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var noNotePlaceholder: some View {
        VStack(spacing: 14) {
            BikeScene(palette: pal, riders: false)
                .frame(width: 100, height: 100 * BikeArt.sceneSize.height / BikeArt.sceneSize.width)
                .opacity(0.85)
            VStack(spacing: 3) {
                Text("No note selected")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(pal.ink)
                Text("Pick a note in the tree, or create one.")
                    .font(.system(size: 12))
                    .foregroundStyle(pal.inkDim)
            }
            newNoteButton
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var newNoteButton: some View {
        Button {
            model.newNote()
        } label: {
            Text("New note")
                .font(.system(size: 12.5, weight: .semibold))
                .foregroundStyle(pal.paper)
                .padding(.horizontal, 14)
                .frame(height: 30)
                .background(RoundedRectangle(cornerRadius: Radius.control).fill(pal.dustyBlue))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
