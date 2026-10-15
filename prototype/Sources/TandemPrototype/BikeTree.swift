import SwiftUI
import Observation

// PROTOTYPE — the bikeshed sidebar: nested file tree (folders + .md notes,
// attachments invisible — ticket 04) and, in the "in tree" search style, the
// search field + full-vault results that replace the tree while typing.

struct BikeTreeView: View {
    @Bindable var model: BikeModel
    var pal: Palette
    var searchInTree: Bool

    var body: some View {
        VStack(spacing: 0) {
            // sidebar header: label + new-note
            HStack(spacing: 6) {
                Image(systemName: "folder")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(pal.inkDim)
                Text("Bikeshed")
                    .font(.system(size: 12.5, weight: .semibold))
                    .foregroundStyle(pal.ink)
                Spacer()
                Button {
                    model.newNote()
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(pal.inkDim)
                        .frame(width: 20, height: 20)
                        .background(RoundedRectangle(cornerRadius: Radius.checkbox)
                            .strokeBorder(pal.ink, lineWidth: Line.hairline))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help("New note")
            }
            .padding(.horizontal, 12)
            .padding(.top, 10)
            .padding(.bottom, 8)

            Rectangle().fill(pal.sand).frame(height: Line.hairline)

            // in-tree search (style .tree): the field lives at the top of the
            // sidebar; typing swaps the tree for full-vault results.
            if searchInTree {
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
                .padding(.horizontal, 10)
                .padding(.top, 8)
            }

            Group {
                if model.searchActive {
                    BikeSearchResults(model: model, pal: pal, compact: true)
                } else {
                    tree
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private var tree: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(model.treeChildren(of: "")) { item in
                    node(item, depth: 0)
                }
            }
            .padding(.vertical, 6)
        }
    }

    /// AnyView: the tree recurses, and `some View` can't name itself.
    private func node(_ item: BikeTreeItem, depth: Int) -> AnyView {
        switch item.kind {
        case .folder:
            return AnyView(
                Group {
                    rowButton {
                        HStack(spacing: 5) {
                            Image(systemName: model.expanded.contains(item.path) ? "chevron.down" : "chevron.right")
                                .font(.system(size: 8, weight: .semibold))
                                .foregroundStyle(pal.inkDim)
                                .frame(width: 9)
                            Image(systemName: "folder")
                                .font(.system(size: 10.5))
                                .foregroundStyle(pal.inkDim)
                            Text(item.name)
                                .font(.system(size: 12.5, weight: .medium))
                                .foregroundStyle(pal.ink)
                        }
                    } action: {
                        model.toggleFolder(item.path)
                    }
                    .padding(.leading, 10 + CGFloat(depth) * 14)
                    if model.expanded.contains(item.path) {
                        ForEach(model.treeChildren(of: item.path)) { child in
                            node(child, depth: depth + 1)
                        }
                    }
                }
            )
        case .note:
            return AnyView(
                rowButton {
                    HStack(spacing: 5) {
                        Image(systemName: "doc.text")
                            .font(.system(size: 10.5))
                            .foregroundStyle(selected(item.path) ? pal.paper.opacity(0.8) : pal.inkDim)
                        Text(item.name)
                            .font(.system(size: 12.5))
                            .foregroundStyle(selected(item.path) ? pal.paper : pal.ink)
                            .lineLimit(1)
                    }
                } action: {
                    model.open(item.path)
                }
                .padding(.leading, 10 + CGFloat(depth) * 14 + 14)
                .background(selected(item.path) ? pal.dustyBlue : Color.clear)
            )
        }
    }

    private func selected(_ path: String) -> Bool { model.openPath == path }

    private func rowButton(label: () -> some View,
                           action: @escaping () -> Void) -> some View {
        Button(action: action) {
            label()
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// MARK: - search results (shared by all three styles)

/// Full-vault results for the model's query. Provisional semantics
/// (ticket 02 settles these): case-insensitive substring over title + body,
/// one row per note, sorted by vault path; title hits lead with the note's
/// first line, body hits lead with a snippet around the first match.
struct BikeSearchResults: View {
    var model: BikeModel
    var pal: Palette
    var compact: Bool  // sidebar rows are tighter than bar/panel rows

    var body: some View {
        let results = model.results
        Group {
            if results.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("No matches")
                        .font(.system(size: 12.5, weight: .medium))
                        .foregroundStyle(pal.ink)
                    Text("Nothing in the vault matches “\(model.query)”.")
                        .font(.system(size: 11.5))
                        .foregroundStyle(pal.inkDim)
                        .lineLimit(2)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(results) { r in
                            Button {
                                model.openSearchResult(r)
                            } label: {
                                VStack(alignment: .leading, spacing: 2) {
                                    HStack(spacing: 5) {
                                        Image(systemName: "doc.text")
                                            .font(.system(size: 10))
                                            .foregroundStyle(pal.inkDim)
                                        Text(r.note.title)
                                            .font(.system(size: compact ? 12.5 : 13, weight: .medium))
                                            .foregroundStyle(pal.ink)
                                            .lineLimit(1)
                                    }
                                    Text(r.snippet)
                                        .font(.system(size: 11, design: .monospaced))
                                        .foregroundStyle(pal.inkDim)
                                        .lineLimit(1)
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, compact ? 12 : 10)
                                .padding(.vertical, compact ? 5 : 7)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            if r.id != results.last?.id {
                                Rectangle().fill(pal.sand).frame(height: Line.hairline)
                                    .padding(.leading, compact ? 24 : 22)
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
