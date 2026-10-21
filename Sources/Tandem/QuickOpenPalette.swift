import SwiftUI
import TandemCore

/// The quick-open palette (⌘K): **one ranked list over the bikeshed's notes and the
/// store's tickets** (ticket 3E1F4D23).
///
/// The list is drawn by the core (`QuickOpen.rank`) — the palette only shows it, moves the
/// selection, and hands the chosen item back. A note opens in the editor; a ticket opens
/// its sheet. The window owns both, so the palette never changes a tab or presents
/// anything itself.
///
/// The register is the rest of the app: flat paper, one hairline, a parchment field, and
/// the selection as a soft wash — no shadow, no gloss.
struct QuickOpenPalette: View {
    /// The candidates: the bikeshed's notes and the store's tickets.
    var items: [QuickOpen.Item]
    var palette: Palette
    /// Opens the chosen item — a note in the editor, a ticket in its sheet.
    var onOpen: (QuickOpen.Item) -> Void
    /// Closes the palette without opening anything.
    var onDismiss: () -> Void

    @State private var query = ""
    /// The highlighted row. Always a row of the ranked list.
    @State private var selection = 0
    @FocusState private var fieldFocused: Bool

    /// How many rows the panel shows before the list starts scrolling. Below it the
    /// panel is exactly as tall as its rows; above it the list scrolls.
    private let maxRowsWithoutScrolling = 8

    /// The ranked list — the core's rule, not the view's.
    private var ranked: [QuickOpen.Item] { QuickOpen.rank(items, query: query) }

    var body: some View {
        VStack(spacing: 0) {
            field
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            results
            Rectangle().fill(palette.sand).frame(height: Line.hairline)
            footer
        }
        .frame(width: 520)
        .background(palette.paper)
        .clipShape(RoundedRectangle(cornerRadius: Radius.control))
        .overlay(RoundedRectangle(cornerRadius: Radius.control)
            .strokeBorder(palette.line, lineWidth: Line.hairline))
        .onAppear { fieldFocused = true }
        // A new query is a new list: the first row is the one to open.
        .onChange(of: query) { _, _ in selection = 0 }
    }

    // MARK: - The field

    private var field: some View {
        HStack(spacing: 7) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 11))
                .foregroundStyle(palette.inkDim)
            TextField("Search notes and tickets…", text: $query)
                .textFieldStyle(.plain)
                .font(.system(size: 12.5))
                .foregroundStyle(palette.ink)
                .focused($fieldFocused)
                .onSubmit { openSelection() }
                // The arrows move the selection, not the caret: the list is the thing
                // being navigated, and the field is the query for it.
                .onKeyPress(.upArrow) { move(-1); return .handled }
                .onKeyPress(.downArrow) { move(1); return .handled }
                .onKeyPress(.escape) { onDismiss(); return .handled }
        }
        .padding(.horizontal, 11)
        .frame(height: Space.control)
    }

    // MARK: - The list

    @ViewBuilder
    private var results: some View {
        if ranked.isEmpty {
            VStack(alignment: .leading, spacing: 3) {
                Text("No matches")
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(palette.ink)
                Text("No note or ticket matches “\(query)”.")
                    .font(.system(size: 11.5))
                    .foregroundStyle(palette.inkDim)
                    .lineLimit(2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
        } else if ranked.count <= maxRowsWithoutScrolling {
            // A short list: the panel is exactly as tall as its rows.
            VStack(spacing: 0) { rows }
                .padding(.vertical, 4)
        } else {
            ScrollViewReader { proxy in
                ScrollView { rows }
                    .frame(maxHeight: 320)
                    .onChange(of: selection) { _, newValue in
                        withAnimation(Motion.toggle) { proxy.scrollTo(newValue, anchor: .center) }
                    }
            }
        }
    }

    /// The rows of the ranked list.
    private var rows: some View {
        ForEach(ranked.indices, id: \.self) { index in
            row(ranked[index], selected: index == selection)
        }
    }

    /// One row: the item's name, and the context that says what it is — a note's folder,
    /// a ticket's stage and lane.
    private func row(_ item: QuickOpen.Item, selected: Bool) -> some View {
        HStack(spacing: 8) {
            Image(systemName: symbol(for: item))
                .font(.system(size: 10.5))
                .foregroundStyle(palette.inkDim)
                .frame(width: 13)
            Text(item.name)
                .font(.system(size: 12.5, weight: selected ? .medium : .regular))
                .foregroundStyle(item.isDone ? palette.inkDim : palette.ink)
                .strikethrough(item.isDone, color: palette.inkDim)
                .lineLimit(1)
            Spacer(minLength: 8)
            Text(detail(for: item))
                .font(.system(size: 10.5, design: .monospaced))
                .foregroundStyle(palette.inkDim)
                .lineLimit(1)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(selected ? AnyShapeStyle(palette.parchment) : AnyShapeStyle(Color.clear))
        .contentShape(Rectangle())
        .onTapGesture { onOpen(item) }
    }

    /// The row's context line.
    private func detail(for item: QuickOpen.Item) -> String {
        switch item {
        case .note(let path):
            return BikeshedPath.folder(of: path)
        case .ticket(let ticket):
            guard let lane = ticket.lane else { return ticket.status.displayName }
            return "\(ticket.status.displayName) · \(lane)"
        }
    }

    /// The row's mark: a note is a document, a ticket is work.
    private func symbol(for item: QuickOpen.Item) -> String {
        switch item {
        case .note: return "doc.text"
        case .ticket: return "checklist"
        }
    }

    // MARK: - The footer

    private var footer: some View {
        HStack(spacing: 0) {
            Text("↑↓ move · ↩ open · esc dismiss")
                .font(.system(size: 10.5))
                .foregroundStyle(palette.inkDim)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .frame(height: 26)
    }

    // MARK: - The selection

    /// Moves the selection by `delta` rows, clamped to the list.
    private func move(_ delta: Int) {
        guard !ranked.isEmpty else { return }
        selection = min(max(0, selection + delta), ranked.count - 1)
    }

    /// Opens the highlighted row, if there is one.
    private func openSelection() {
        guard ranked.indices.contains(selection) else { return }
        onOpen(ranked[selection])
    }
}
