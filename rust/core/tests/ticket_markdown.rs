//! The ticket faces' markdown seam (TAN-219), ported from
//! `TicketMarkdownTests.swift`: pure strings in, blocks out. The views draw what
//! this returns, and nothing here knows a view.

mod support;

use tandem_core::domain::{MarkdownBlock, MarkdownInline, TicketMarkdown};

fn run(text: &str) -> MarkdownInline {
    MarkdownInline::new(text)
}

fn styled(text: &str, styles: &[tandem_core::domain::InlineStyle]) -> MarkdownInline {
    MarkdownInline {
        styles: styles.iter().copied().collect(),
        ..MarkdownInline::new(text)
    }
}

fn linked(text: &str, link: &str) -> MarkdownInline {
    MarkdownInline {
        link: Some(link.to_string()),
        ..MarkdownInline::new(text)
    }
}

fn paragraph(text: &str) -> MarkdownBlock {
    MarkdownBlock::Paragraph {
        runs: vec![run(text)],
    }
}

#[test]
fn headings_keep_their_level_and_lose_their_hashes() {
    assert_eq!(
        TicketMarkdown::blocks(Some("## Why\n\nthe reason")),
        [
            MarkdownBlock::Heading {
                level: 2,
                runs: vec![run("Why")]
            },
            paragraph("the reason"),
        ]
    );
}

#[test]
fn a_bullet_list_is_item_blocks_and_a_nested_list_is_a_child_block() {
    assert_eq!(
        TicketMarkdown::blocks(Some("- one\n- two\n  - nested")),
        [MarkdownBlock::List {
            ordered: false,
            start: 1,
            items: vec![
                vec![paragraph("one")],
                vec![
                    paragraph("two"),
                    MarkdownBlock::List {
                        ordered: false,
                        start: 1,
                        items: vec![vec![paragraph("nested")]],
                    },
                ],
            ],
        }]
    );
}

#[test]
fn an_ordered_list_keeps_its_start_number() {
    assert_eq!(
        TicketMarkdown::blocks(Some("3. three\n4. four")),
        [MarkdownBlock::List {
            ordered: true,
            start: 3,
            items: vec![vec![paragraph("three")], vec![paragraph("four")]],
        }]
    );
}

#[test]
fn a_fenced_block_keeps_its_language_and_its_text_verbatim() {
    assert_eq!(
        TicketMarkdown::blocks(Some("```swift\nlet x = 1\nprint(x)\n```")),
        [MarkdownBlock::Code {
            language: Some("swift".to_string()),
            text: "let x = 1\nprint(x)".to_string(),
        }]
    );
    // A fence with no info string has no language.
    assert_eq!(
        TicketMarkdown::blocks(Some("```\nplain\n```")),
        [MarkdownBlock::Code {
            language: None,
            text: "plain".to_string()
        }]
    );
}

#[test]
fn inline_code_bold_italic_and_strikethrough_become_styles_not_characters() {
    use tandem_core::domain::InlineStyle::{Bold, Code, Italic, Strikethrough};
    assert_eq!(
        TicketMarkdown::blocks(Some("use `code`, **bold**, *italic* and ~~gone~~")),
        [MarkdownBlock::Paragraph {
            runs: vec![
                run("use "),
                styled("code", &[Code]),
                run(", "),
                styled("bold", &[Bold]),
                run(", "),
                styled("italic", &[Italic]),
                run(" and "),
                styled("gone", &[Strikethrough]),
            ]
        }]
    );
    // Nested markers carry both styles on the span they cover.
    assert_eq!(
        TicketMarkdown::blocks(Some("**bold with `code`**")),
        [MarkdownBlock::Paragraph {
            runs: vec![styled("bold with ", &[Bold]), styled("code", &[Bold, Code])]
        }]
    );
}

#[test]
fn a_link_keeps_its_label_and_its_destination() {
    assert_eq!(
        TicketMarkdown::blocks(Some("read [the brief](https://example.com/brief) now")),
        [MarkdownBlock::Paragraph {
            runs: vec![
                run("read "),
                linked("the brief", "https://example.com/brief"),
                run(" now"),
            ]
        }]
    );
}

#[test]
fn a_table_is_a_table() {
    assert_eq!(
        TicketMarkdown::blocks(Some(
            "| what | state |\n| --- | --- |\n| seam | in |\n| view | out |"
        )),
        [MarkdownBlock::Table {
            header: vec!["what".to_string(), "state".to_string()],
            rows: vec![
                vec!["seam".to_string(), "in".to_string()],
                vec!["view".to_string(), "out".to_string()],
            ],
        }]
    );
}

#[test]
fn a_blockquote_nests_its_blocks() {
    assert_eq!(
        TicketMarkdown::blocks(Some("> quiet\n>\n> still quiet")),
        [MarkdownBlock::Quote(vec![
            paragraph("quiet"),
            paragraph("still quiet")
        ])]
    );
}

#[test]
fn a_thematic_break_is_a_rule() {
    assert_eq!(TicketMarkdown::blocks(Some("---")), [MarkdownBlock::Rule]);
}

#[test]
fn a_task_item_keeps_its_checkbox() {
    assert_eq!(
        TicketMarkdown::blocks(Some("- [ ] do it\n- [x] did it")),
        [MarkdownBlock::List {
            ordered: false,
            start: 1,
            items: vec![
                vec![MarkdownBlock::Task {
                    checked: false,
                    blocks: vec![paragraph("do it")],
                }],
                vec![MarkdownBlock::Task {
                    checked: true,
                    blocks: vec![paragraph("did it")],
                }],
            ],
        }]
    );
}

#[test]
fn an_image_renders_as_its_alt_text_and_raw_html_stays_literal() {
    assert_eq!(
        TicketMarkdown::blocks(Some(
            "![a picture](https://example.com/p.png)\n\n<div>raw</div>"
        )),
        [
            paragraph("a picture"),
            MarkdownBlock::Code {
                language: None,
                text: "<div>raw</div>".to_string(),
            },
        ]
    );
}

#[test]
fn empty_whitespace_and_absent_bodies_parse_to_no_blocks() {
    assert_eq!(
        TicketMarkdown::blocks(Some("")),
        Vec::<MarkdownBlock>::new()
    );
    assert_eq!(
        TicketMarkdown::blocks(Some("  \n\t\n ")),
        Vec::<MarkdownBlock>::new()
    );
    assert_eq!(TicketMarkdown::blocks(None), Vec::<MarkdownBlock>::new());
}

#[test]
fn the_excerpt_is_the_first_paragraph_not_the_heading() {
    let source = "## Why\n\nThe reason the ticket exists.\n\nMore below.";
    assert_eq!(
        TicketMarkdown::excerpt(Some(source)).as_deref(),
        Some("The reason the ticket exists.")
    );
}

#[test]
fn the_excerpt_looks_inside_a_list_item_and_a_quote() {
    assert_eq!(
        TicketMarkdown::excerpt(Some("- the first item\n- the second")).as_deref(),
        Some("the first item")
    );
    assert_eq!(
        TicketMarkdown::excerpt(Some("> quoted opening")).as_deref(),
        Some("quoted opening")
    );
}

#[test]
fn the_excerpt_collapses_whitespace_and_drops_inline_markers() {
    let source =
        "## Why\n\nThe   reason\nis **bold** and `code`, see [the brief](https://example.com).";
    assert_eq!(
        TicketMarkdown::excerpt(Some(source)).as_deref(),
        Some("The reason is bold and code, see the brief.")
    );
}

#[test]
fn no_prose_means_no_excerpt() {
    assert_eq!(TicketMarkdown::excerpt(None), None);
    assert_eq!(TicketMarkdown::excerpt(Some("")), None);
    assert_eq!(TicketMarkdown::excerpt(Some("   \n\n ")), None);
    assert_eq!(
        TicketMarkdown::excerpt(Some("## Blocked on\n\n```\ncode\n```\n\n---")),
        None
    );
    assert_eq!(TicketMarkdown::excerpt(Some("## Blocked on")), None);
}
