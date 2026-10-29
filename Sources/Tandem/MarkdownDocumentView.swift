import SwiftUI
import TandemCore

/// A ticket's description or one of its comments, drawn for reading (TAN-219).
///
/// It draws what `TicketMarkdown.blocks` hands back and nothing else: the sheet owns
/// the source, the toggle back to it, and the write. Every size and colour comes from
/// the sheet's own scale and `Palette`, so the view follows the day/night panel without
/// a literal of its own, and `base` is the sheet's two sizes: 12.5 for the description,
/// 12 for a comment.
struct MarkdownDocumentView: View {
    var blocks: [MarkdownBlock]
    var palette: Palette
    var base: CGFloat = 12.5

    var body: some View {
        MarkdownBlocksView(blocks: blocks, palette: palette, base: base,
                           color: palette.ink, depth: 0)
            .textSelection(.enabled)
    }
}

/// The recursive half: one container per block list, so a nested list, a quote or a
/// task item's blocks draw themselves at the next depth.
private struct MarkdownBlocksView: View {
    let blocks: [MarkdownBlock]
    let palette: Palette
    let base: CGFloat
    let color: Color
    let depth: Int

    var body: some View {
        VStack(alignment: .leading, spacing: base * 0.55) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                blockView(block)
            }
        }
    }

    @ViewBuilder
    private func blockView(_ block: MarkdownBlock) -> some View {
        switch block {
        case let .heading(level, runs):
            Text(attributed(runs, size: headingSize(level), weight: .semibold))
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, level <= 2 ? base * 0.3 : base * 0.15)

        case let .paragraph(runs):
            Text(attributed(runs, size: base))
                .fixedSize(horizontal: false, vertical: true)

        case let .list(ordered, start, items):
            VStack(alignment: .leading, spacing: base * 0.35) {
                ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                    HStack(alignment: .firstTextBaseline, spacing: base * 0.45) {
                        Text(marker(ordered: ordered, number: start + index))
                            .font(.system(size: base, design: .monospaced))
                            .foregroundStyle(palette.inkDim)
                            .frame(minWidth: base * 1.1, alignment: .trailing)
                        MarkdownBlocksView(blocks: item, palette: palette, base: base,
                                           color: color, depth: depth + 1)
                    }
                }
            }
            .padding(.leading, base * 0.15)

        case let .task(checked, blocks):
            HStack(alignment: .firstTextBaseline, spacing: base * 0.45) {
                Text(checked ? "☑" : "☐")
                    .font(.system(size: base))
                    .foregroundStyle(checked ? palette.mossDeep : palette.inkDim)
                MarkdownBlocksView(blocks: blocks, palette: palette, base: base,
                                   color: color, depth: depth + 1)
            }

        case let .code(language, text):
            VStack(alignment: .leading, spacing: 3) {
                if let language, !language.isEmpty {
                    Text(language)
                        .font(.system(size: base - 1.5, design: .monospaced))
                        .foregroundStyle(palette.inkDim)
                }
                Text(text)
                    .font(.system(size: base - 0.5, design: .monospaced))
                    .foregroundStyle(palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: Radius.control).fill(palette.parchment))
            .overlay(RoundedRectangle(cornerRadius: Radius.control)
                .strokeBorder(palette.line, lineWidth: Line.hairline))

        case let .quote(blocks):
            MarkdownBlocksView(blocks: blocks, palette: palette, base: base,
                               color: palette.inkDim, depth: depth)
                .padding(.leading, 8)
                .overlay(alignment: .leading) {
                    Rectangle().fill(palette.line).frame(width: Line.hairline)
                }

        case let .table(header, rows):
            VStack(alignment: .leading, spacing: 3) {
                Text(header.joined(separator: " │ "))
                    .font(.system(size: base - 1, weight: .medium, design: .monospaced))
                    .foregroundStyle(palette.ink)
                Rectangle().fill(palette.sand).frame(height: Line.hairline)
                ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                    Text(row.joined(separator: " │ "))
                        .font(.system(size: base - 1, design: .monospaced))
                        .foregroundStyle(color)
                }
            }
            .fixedSize(horizontal: false, vertical: true)

        case .rule:
            Rectangle()
                .fill(palette.sand)
                .frame(height: Line.hairline)
                .padding(.vertical, 2)
        }
    }

    // MARK: - Inline runs

    /// The runs as one attributed string: each span keeps its own font, colour and
    /// strike, so a bold span stays bold inside a paragraph of plain ones. An attributed
    /// string rather than concatenated `Text` because that concatenation is deprecated.
    private func attributed(_ runs: [MarkdownInline], size: CGFloat,
                            weight: Font.Weight = .regular) -> AttributedString {
        var result = AttributedString()
        for run in runs {
            var piece = AttributedString(run.text)
            let isCode = run.styles.contains(.code)
            var font = Font.system(size: isCode ? size - 0.5 : size,
                                   weight: run.styles.contains(.bold) ? .bold : weight,
                                   design: isCode ? .monospaced : .default)
            if run.styles.contains(.italic) { font = font.italic() }
            piece.font = font
            if run.styles.contains(.strikethrough) { piece.strikethroughStyle = .single }
            // A link draws its label, not its URL: the destination is the run's own
            // property, and the system opens it where a link is clickable (TAN-219 keeps
            // wikilinks and ticket refs out of this).
            piece.foregroundColor = run.link == nil ? color : palette.emberDeep
            result += piece
        }
        return result
    }

    // MARK: - Markers

    /// A heading's size: semibold at base +4 / +2 / +1 by level, then base.
    private func headingSize(_ level: Int) -> CGFloat {
        switch level {
        case 1: base + 4
        case 2: base + 2
        case 3: base + 1
        default: base
        }
    }

    private func marker(ordered: Bool, number: Int) -> String {
        ordered ? "\(number)." : "•"
    }
}
