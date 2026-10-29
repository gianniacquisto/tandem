import Foundation
import Markdown

/// A ticket's description or comment, parsed for reading (TAN-219).
///
/// The sheet used to hand markdown back as the stored bytes: `## Why`, `**bold**` and
/// backticks read literally, on the screen the human reads most. This seam turns those
/// bytes into a small view-model the app draws: one block per paragraph, heading, list,
/// code block, quote, table or rule, and one run per styled span inside them. The
/// source stays the source: the sheet's `Edit source` shows the exact stored bytes,
/// and `Save` writes them unchanged.
///
/// It rides the `swift-markdown` dependency the core already carries for the bikeshed
/// index (`BikeshedOperations`), so the ticket faces read the same GFM dialect the
/// editor highlights: tables, strikethrough and task lists included. The view-model is
/// `Sendable` and `Equatable` on purpose: it crosses the core's thread boundary and it
/// is the tests' subject.
public enum TicketMarkdown {

    // MARK: - Blocks

    /// The document's blocks, top level first. An empty, whitespace-only or `nil`
    /// body parses to no blocks, which is how the sheet tells a body to read from
    /// one to write.
    public static func blocks(_ source: String?) -> [MarkdownBlock] {
        guard let source,
              !source.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        else { return [] }
        return convertChildren(of: Document(parsing: source))
    }

    /// A plain-text excerpt: the first paragraph in document order, looking inside a
    /// list item or a quote, with the heading's own text skipped and whitespace
    /// collapsed. `nil` when the body carries no prose (only headings, code, tables or
    /// rules); the board row and tile then draw no second line.
    public static func excerpt(_ source: String?) -> String? {
        guard let source, !source.isEmpty else { return nil }
        return firstParagraph(of: Document(parsing: source).children)
    }

    // MARK: - Block conversion

    private static func convertChildren(of container: some BlockContainer) -> [MarkdownBlock] {
        container.children.compactMap(convert(_:))
    }

    private static func convert(_ markup: Markup) -> MarkdownBlock? {
        switch markup {
        case let heading as Heading:
            .heading(level: heading.level, runs: runs(of: heading))
        case let paragraph as Paragraph:
            .paragraph(runs: runs(of: paragraph))
        case let list as UnorderedList:
            .list(ordered: false, start: 1, items: items(of: list))
        case let list as OrderedList:
            .list(ordered: true, start: Int(list.startIndex), items: items(of: list))
        case let code as CodeBlock:
            // The parser's literal carries the fence's trailing newline; the block's
            // text is the lines the author wrote, so one trailing newline is dropped.
            .code(language: code.language, text: droppingTrailingNewline(code.code))
        case let quote as BlockQuote:
            .quote(convertChildren(of: quote))
        case let table as Table:
            .table(header: table.head.cells.map(cellText),
                   rows: table.body.rows.map { $0.cells.map(cellText) })
        case is ThematicBreak:
            .rule
        case let html as HTMLBlock:
            // The app loads no remote content and draws no HTML: a raw block is
            // literal text, monospaced like code, so nothing is silently swallowed.
            .code(language: nil, text: droppingTrailingNewline(html.rawHTML))
        default:
            nil
        }
    }

    /// A list's items, each a list of blocks. A nested list is one of the item's child
    /// blocks, so the view's indent follows depth without a second model; a task item
    /// carries its checkbox in a `.task` block over the same children.
    private static func items(of list: some ListItemContainer) -> [[MarkdownBlock]] {
        list.listItems.map { item in
            let blocks = convertChildren(of: item)
            guard let checkbox = item.checkbox else { return blocks }
            return [.task(checked: checkbox == .checked, blocks: blocks)]
        }
    }

    /// A table cell's text: the runs flattened, markers dropped, whitespace collapsed.
    /// Alignment and inline styling inside cells are not modelled (TAN-219).
    private static func cellText(_ cell: Table.Cell) -> String {
        let text = runs(of: cell).map(\.text).joined()
        return collapsingWhitespace(text)
    }

    // MARK: - Inline conversion

    private static func runs(of markup: Markup) -> [MarkdownInline] {
        var runs: [MarkdownInline] = []
        collect(markup, styles: [], link: nil, into: &runs)
        return mergingAdjacent(runs)
    }

    private static func collect(_ markup: Markup,
                                styles: Set<InlineStyle>,
                                link: String?,
                                into runs: inout [MarkdownInline]) {
        switch markup {
        case let text as Text:
            runs.append(MarkdownInline(text: text.string, styles: styles, link: link))
        case let code as InlineCode:
            runs.append(MarkdownInline(text: code.code,
                                       styles: styles.union([.code]), link: link))
        case let html as InlineHTML:
            runs.append(MarkdownInline(text: html.rawHTML, styles: styles, link: link))
        case is SoftBreak:
            runs.append(MarkdownInline(text: " ", styles: styles, link: link))
        case is LineBreak:
            runs.append(MarkdownInline(text: "\n", styles: styles, link: link))
        case let strong as Strong:
            collectChildren(strong, styles: styles.union([.bold]), link: link, into: &runs)
        case let emphasis as Emphasis:
            collectChildren(emphasis, styles: styles.union([.italic]), link: link, into: &runs)
        case let strike as Strikethrough:
            collectChildren(strike, styles: styles.union([.strikethrough]), link: link, into: &runs)
        case let linkNode as Link:
            collectChildren(linkNode, styles: styles,
                            link: linkNode.destination ?? link, into: &runs)
        case is Image:
            // The alt text is the image: the app loads no remote image, and pictures
            // belong to the bikeshed editor.
            let image = markup as! Image
            collectChildren(image, styles: styles, link: nil, into: &runs)
        default:
            collectChildren(markup, styles: styles, link: link, into: &runs)
        }
    }

    private static func collectChildren(_ markup: Markup,
                                        styles: Set<InlineStyle>,
                                        link: String?,
                                        into runs: inout [MarkdownInline]) {
        for child in markup.children {
            collect(child, styles: styles, link: link, into: &runs)
        }
    }

    /// Adjacent runs with the same styles and destination are one run: the parser
    /// splits a span around an entity or an escaped character, and the view should not
    /// see two runs where the author wrote one.
    private static func mergingAdjacent(_ runs: [MarkdownInline]) -> [MarkdownInline] {
        var merged: [MarkdownInline] = []
        for run in runs {
            if let last = merged.last, last.styles == run.styles, last.link == run.link {
                merged[merged.count - 1].text += run.text
            } else {
                merged.append(run)
            }
        }
        return merged
    }

    // MARK: - Excerpt

    private static func firstParagraph(of children: MarkupChildren) -> String? {
        for child in children {
            if let paragraph = child as? Paragraph {
                let text = collapsingWhitespace(runs(of: paragraph).map(\.text).joined())
                if !text.isEmpty { return text }
            } else if let quote = child as? BlockQuote {
                if let text = firstParagraph(of: quote.children) { return text }
            } else if let list = child as? ListItemContainer {
                for item in list.listItems {
                    if let text = firstParagraph(of: item.children) { return text }
                }
            }
        }
        return nil
    }

    // MARK: - Text

    private static func collapsingWhitespace(_ text: String) -> String {
        text.split(whereSeparator: \.isWhitespace).joined(separator: " ")
    }

    private static func droppingTrailingNewline(_ text: String) -> String {
        text.hasSuffix("\n") ? String(text.dropLast()) : text
    }
}

/// One block of a ticket's markdown, ready to draw. A list item is a list of blocks,
/// so a nested list is a child block and the view's indent is the depth.
public enum MarkdownBlock: Sendable, Equatable {
    /// `## Why`: the hashes are gone, the level is kept.
    case heading(level: Int, runs: [MarkdownInline])
    /// A run of prose.
    case paragraph(runs: [MarkdownInline])
    /// A bullet or numbered list. `start` is the first item's number as authored; a
    /// bullet list always starts at 1.
    case list(ordered: Bool, start: Int, items: [[MarkdownBlock]])
    /// A fenced or indented block, its info string as the language.
    case code(language: String?, text: String)
    /// A blockquote's own blocks.
    case quote([MarkdownBlock])
    /// A GFM table: the header row and the body rows, cells as plain text.
    case table(header: [String], rows: [[String]])
    /// A thematic break (`---`).
    case rule
    /// A task list item (`- [ ]`): the checkbox over the item's own blocks.
    case task(checked: Bool, blocks: [MarkdownBlock])
}

/// One styled span of inline text. `styles` is a set because markers nest
/// (`**bold with `code`**` is one run carrying both), and `link` is the destination of
/// `[label](url)`; the view draws the label, not the URL.
public struct MarkdownInline: Sendable, Equatable {
    public var text: String
    public var styles: Set<InlineStyle>
    public var link: String?

    public init(text: String, styles: Set<InlineStyle> = [], link: String? = nil) {
        self.text = text
        self.styles = styles
        self.link = link
    }
}

/// The inline markers a run can carry. They are styles, never characters: `**x**`
/// reaches the view as the text `x` with `.bold`.
public enum InlineStyle: Sendable, Equatable, Hashable {
    case bold
    case italic
    case strikethrough
    case code
}
