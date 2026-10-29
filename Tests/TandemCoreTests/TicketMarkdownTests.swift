import Foundation
import Testing
@testable import TandemCore

/// The ticket faces' markdown seam (TAN-219): pure strings in, blocks out. The views
/// draw what this returns, and nothing here knows SwiftUI.
@Suite("Ticket markdown")
struct TicketMarkdownTests {

    // MARK: - Blocks

    @Test("headings keep their level and lose their hashes")
    func headings() {
        #expect(TicketMarkdown.blocks("## Why\n\nthe reason") == [
            .heading(level: 2, runs: [MarkdownInline(text: "Why")]),
            .paragraph(runs: [MarkdownInline(text: "the reason")]),
        ])
    }

    @Test("a bullet list is item blocks and a nested list is a child block")
    func bulletList() {
        let source = "- one\n- two\n  - nested"
        #expect(TicketMarkdown.blocks(source) == [
            .list(ordered: false, start: 1, items: [
                [.paragraph(runs: [MarkdownInline(text: "one")])],
                [
                    .paragraph(runs: [MarkdownInline(text: "two")]),
                    .list(ordered: false, start: 1, items: [
                        [.paragraph(runs: [MarkdownInline(text: "nested")])],
                    ]),
                ],
            ]),
        ])
    }

    @Test("an ordered list keeps its start number")
    func orderedList() {
        let source = "3. three\n4. four"
        #expect(TicketMarkdown.blocks(source) == [
            .list(ordered: true, start: 3, items: [
                [.paragraph(runs: [MarkdownInline(text: "three")])],
                [.paragraph(runs: [MarkdownInline(text: "four")])],
            ]),
        ])
    }

    @Test("a fenced block keeps its language and its text verbatim")
    func fencedBlock() {
        let source = "```swift\nlet x = 1\nprint(x)\n```"
        #expect(TicketMarkdown.blocks(source) == [
            .code(language: "swift", text: "let x = 1\nprint(x)"),
        ])
        // A fence with no info string has no language.
        #expect(TicketMarkdown.blocks("```\nplain\n```") == [
            .code(language: nil, text: "plain"),
        ])
    }

    @Test("inline code, bold, italic and strikethrough become styles, not characters")
    func inlineStyles() {
        let source = "use `code`, **bold**, *italic* and ~~gone~~"
        #expect(TicketMarkdown.blocks(source) == [
            .paragraph(runs: [
                MarkdownInline(text: "use "),
                MarkdownInline(text: "code", styles: [.code]),
                MarkdownInline(text: ", "),
                MarkdownInline(text: "bold", styles: [.bold]),
                MarkdownInline(text: ", "),
                MarkdownInline(text: "italic", styles: [.italic]),
                MarkdownInline(text: " and "),
                MarkdownInline(text: "gone", styles: [.strikethrough]),
            ]),
        ])
        // Nested markers carry both styles on the span they cover.
        #expect(TicketMarkdown.blocks("**bold with `code`**") == [
            .paragraph(runs: [
                MarkdownInline(text: "bold with ", styles: [.bold]),
                MarkdownInline(text: "code", styles: [.bold, .code]),
            ]),
        ])
    }

    @Test("a link keeps its label and its destination")
    func link() {
        let source = "read [the brief](https://example.com/brief) now"
        #expect(TicketMarkdown.blocks(source) == [
            .paragraph(runs: [
                MarkdownInline(text: "read "),
                MarkdownInline(text: "the brief", link: "https://example.com/brief"),
                MarkdownInline(text: " now"),
            ]),
        ])
    }

    @Test("a table is a table")
    func table() {
        let source = "| what | state |\n| --- | --- |\n| seam | in |\n| view | out |"
        #expect(TicketMarkdown.blocks(source) == [
            .table(header: ["what", "state"], rows: [["seam", "in"], ["view", "out"]]),
        ])
    }

    @Test("a blockquote nests its blocks")
    func blockquote() {
        let source = "> quiet\n>\n> still quiet"
        #expect(TicketMarkdown.blocks(source) == [
            .quote([
                .paragraph(runs: [MarkdownInline(text: "quiet")]),
                .paragraph(runs: [MarkdownInline(text: "still quiet")]),
            ]),
        ])
    }

    @Test("a thematic break is a rule")
    func rule() {
        #expect(TicketMarkdown.blocks("---") == [.rule])
    }

    @Test("a task item keeps its checkbox")
    func taskItem() {
        let source = "- [ ] do it\n- [x] did it"
        #expect(TicketMarkdown.blocks(source) == [
            .list(ordered: false, start: 1, items: [
                [.task(checked: false, blocks: [
                    .paragraph(runs: [MarkdownInline(text: "do it")]),
                ])],
                [.task(checked: true, blocks: [
                    .paragraph(runs: [MarkdownInline(text: "did it")]),
                ])],
            ]),
        ])
    }

    @Test("an image renders as its alt text and raw html stays literal")
    func imageAndRawHTML() {
        let source = "![a picture](https://example.com/p.png)\n\n<div>raw</div>"
        #expect(TicketMarkdown.blocks(source) == [
            .paragraph(runs: [MarkdownInline(text: "a picture")]),
            .code(language: nil, text: "<div>raw</div>"),
        ])
    }

    @Test("empty, whitespace and nil bodies parse to no blocks")
    func emptyBodies() {
        #expect(TicketMarkdown.blocks("") == [])
        #expect(TicketMarkdown.blocks("  \n\t\n ") == [])
        #expect(TicketMarkdown.blocks(nil) == [])
    }

    // MARK: - Excerpt

    @Test("the excerpt is the first paragraph, not the heading")
    func excerptSkipsHeading() {
        let source = "## Why\n\nThe reason the ticket exists.\n\nMore below."
        #expect(TicketMarkdown.excerpt(source) == "The reason the ticket exists.")
    }

    @Test("the excerpt looks inside a list item")
    func excerptLooksInsideList() {
        #expect(TicketMarkdown.excerpt("- the first item\n- the second") == "the first item")
        #expect(TicketMarkdown.excerpt("> quoted opening") == "quoted opening")
    }

    @Test("the excerpt collapses whitespace and drops inline markers")
    func excerptCollapses() {
        let source = "## Why\n\nThe   reason\nis **bold** and `code`, see [the brief](https://example.com)."
        #expect(TicketMarkdown.excerpt(source)
                == "The reason is bold and code, see the brief.")
    }

    @Test("no prose means no excerpt")
    func excerptWithoutProse() {
        #expect(TicketMarkdown.excerpt(nil) == nil)
        #expect(TicketMarkdown.excerpt("") == nil)
        #expect(TicketMarkdown.excerpt("   \n\n ") == nil)
        #expect(TicketMarkdown.excerpt("## Blocked on\n\n```\ncode\n```\n\n---") == nil)
        #expect(TicketMarkdown.excerpt("## Blocked on") == nil)
    }
}
