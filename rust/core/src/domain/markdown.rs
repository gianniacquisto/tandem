//! A ticket's description or comment, parsed for reading (TAN-219,
//! `TicketMarkdown.swift`).
//!
//! The sheet used to hand markdown back as the stored bytes: `## Why`, `**bold**`
//! and backticks read literally. This seam turns those bytes into a small view
//! model the app draws: one block per paragraph, heading, list, code block, quote,
//! table or rule, and one run per styled span inside them. The source stays the
//! source: the sheet's `Edit source` shows the exact stored bytes.
//!
//! It rides `comrak`, the GFM parser the bikeshed's health report uses (PRT-7), so
//! the ticket faces read the same dialect the editor highlights: tables,
//! strikethrough and task lists included. The view model is `PartialEq` on purpose:
//! it is the tests' subject.

use std::collections::BTreeSet;

use comrak::nodes::{AstNode, ListType, NodeValue};
use comrak::{Options, parse_document};

/// The ticket faces' markdown seam: pure strings in, blocks out.
pub struct TicketMarkdown;

impl TicketMarkdown {
    /// The document's blocks, top level first. An empty, whitespace-only or absent
    /// body parses to no blocks, which is how the sheet tells a body to read from
    /// one to write.
    pub fn blocks(source: Option<&str>) -> Vec<MarkdownBlock> {
        let Some(source) = source else {
            return Vec::new();
        };
        if source.trim().is_empty() {
            return Vec::new();
        }
        let arena = comrak::Arena::new();
        let document = parse_document(&arena, source, &options());
        children(document)
    }

    /// A plain-text excerpt: the first paragraph in document order, looking inside
    /// a list item or a quote, with the heading's own text skipped and whitespace
    /// collapsed. `None` when the body carries no prose.
    pub fn excerpt(source: Option<&str>) -> Option<String> {
        let source = source.filter(|source| !source.is_empty())?;
        let arena = comrak::Arena::new();
        let document = parse_document(&arena, source, &options());
        first_paragraph(document)
    }
}

/// The GFM dialect the editor highlights: tables, strikethrough, task lists.
fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options
}

/// One block of a ticket's markdown, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownBlock {
    /// `## Why`: the hashes are gone, the level is kept.
    Heading {
        level: usize,
        runs: Vec<MarkdownInline>,
    },
    /// A run of prose.
    Paragraph { runs: Vec<MarkdownInline> },
    /// A bullet or numbered list. `start` is the first item's number as authored; a
    /// bullet list always starts at 1.
    List {
        ordered: bool,
        start: i64,
        items: Vec<Vec<MarkdownBlock>>,
    },
    /// A fenced or indented block, its info string as the language.
    Code {
        language: Option<String>,
        text: String,
    },
    /// A blockquote's own blocks.
    Quote(Vec<MarkdownBlock>),
    /// A GFM table: the header row and the body rows, cells as plain text.
    Table {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// A thematic break (`---`).
    Rule,
    /// A task list item (`- [ ]`): the checkbox over the item's own blocks.
    Task {
        checked: bool,
        blocks: Vec<MarkdownBlock>,
    },
}

/// One styled span of inline text. `styles` is a set because markers nest
/// (`**bold with `code`**` is one run carrying both), and `link` is the
/// destination of `[label](url)`; the view draws the label, not the URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownInline {
    pub text: String,
    pub styles: BTreeSet<InlineStyle>,
    pub link: Option<String>,
}

impl MarkdownInline {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            styles: BTreeSet::new(),
            link: None,
        }
    }
}

/// The inline markers a run can carry. They are styles, never characters: `**x**`
/// reaches the view as the text `x` with [`InlineStyle::Bold`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InlineStyle {
    Bold,
    Italic,
    Strikethrough,
    Code,
}

fn children<'a>(node: &'a AstNode<'a>) -> Vec<MarkdownBlock> {
    node.children().filter_map(convert).collect()
}

fn convert<'a>(node: &'a AstNode<'a>) -> Option<MarkdownBlock> {
    let block = {
        let data = node.data.borrow();
        match &data.value {
            NodeValue::Heading(heading) => MarkdownBlock::Heading {
                level: usize::from(heading.level),
                runs: runs(node),
            },
            NodeValue::Paragraph => MarkdownBlock::Paragraph { runs: runs(node) },
            NodeValue::List(list) => {
                let ordered = list.list_type == ListType::Ordered;
                MarkdownBlock::List {
                    ordered,
                    start: if ordered { list.start as i64 } else { 1 },
                    items: items(node),
                }
            }
            NodeValue::CodeBlock(code) => MarkdownBlock::Code {
                language: language(&code.info),
                text: dropping_trailing_newline(&code.literal),
            },
            NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => {
                MarkdownBlock::Quote(children(node))
            }
            NodeValue::Table(_) => table(node),
            NodeValue::ThematicBreak => MarkdownBlock::Rule,
            // The app loads no remote content and draws no HTML: a raw block is
            // literal text, monospaced like code, so nothing is silently swallowed.
            NodeValue::HtmlBlock(html) => MarkdownBlock::Code {
                language: None,
                text: dropping_trailing_newline(&html.literal),
            },
            NodeValue::TaskItem(task) => MarkdownBlock::Task {
                checked: task.symbol.is_some(),
                blocks: children(node),
            },
            _ => return None,
        }
    };
    Some(block)
}

/// A list's items, each a list of blocks. A nested list is one of the item's child
/// blocks, so the view's indent follows depth without a second model; a task item
/// carries its checkbox in a `Task` block over the same children.
fn items<'a>(list: &'a AstNode<'a>) -> Vec<Vec<MarkdownBlock>> {
    list.children()
        .map(|item| match &item.data.borrow().value {
            NodeValue::TaskItem(task) => vec![MarkdownBlock::Task {
                checked: task.symbol.is_some(),
                blocks: children(item),
            }],
            _ => children(item),
        })
        .collect()
}

/// A table: the header row and the body rows, each cell flattened to plain text.
fn table<'a>(node: &'a AstNode<'a>) -> MarkdownBlock {
    let mut header: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for row in node.children() {
        let is_header = match &row.data.borrow().value {
            NodeValue::TableRow(header) => *header,
            _ => false,
        };
        let cells: Vec<String> = row.children().map(cell_text).collect();
        if is_header && header.is_empty() {
            header = cells;
        } else {
            rows.push(cells);
        }
    }
    MarkdownBlock::Table { header, rows }
}

/// A table cell's text: the runs flattened, markers dropped, whitespace collapsed.
fn cell_text<'a>(cell: &'a AstNode<'a>) -> String {
    let text = runs(cell)
        .into_iter()
        .map(|run| run.text)
        .collect::<String>();
    collapsing_whitespace(&text)
}

fn runs<'a>(node: &'a AstNode<'a>) -> Vec<MarkdownInline> {
    let mut collected = Vec::new();
    collect(node, &BTreeSet::new(), None, &mut collected);
    merging_adjacent(collected)
}

fn collect<'a>(
    node: &'a AstNode<'a>,
    styles: &BTreeSet<InlineStyle>,
    link: Option<&str>,
    runs: &mut Vec<MarkdownInline>,
) {
    for child in node.children() {
        {
            let data = child.data.borrow();
            match &data.value {
                NodeValue::Text(text) => runs.push(MarkdownInline {
                    text: text.to_string(),
                    styles: styles.clone(),
                    link: link.map(str::to_string),
                }),
                NodeValue::Code(code) => {
                    let mut styles = styles.clone();
                    styles.insert(InlineStyle::Code);
                    runs.push(MarkdownInline {
                        text: code.literal.clone(),
                        styles,
                        link: link.map(str::to_string),
                    });
                }
                NodeValue::HtmlInline(html) => runs.push(MarkdownInline {
                    text: html.clone(),
                    styles: styles.clone(),
                    link: link.map(str::to_string),
                }),
                NodeValue::SoftBreak => runs.push(MarkdownInline {
                    text: " ".to_string(),
                    styles: styles.clone(),
                    link: link.map(str::to_string),
                }),
                NodeValue::LineBreak => runs.push(MarkdownInline {
                    text: "\n".to_string(),
                    styles: styles.clone(),
                    link: link.map(str::to_string),
                }),
                NodeValue::Strong => {
                    let mut styles = styles.clone();
                    styles.insert(InlineStyle::Bold);
                    collect(child, &styles, link, runs);
                }
                NodeValue::Emph => {
                    let mut styles = styles.clone();
                    styles.insert(InlineStyle::Italic);
                    collect(child, &styles, link, runs);
                }
                NodeValue::Strikethrough => {
                    let mut styles = styles.clone();
                    styles.insert(InlineStyle::Strikethrough);
                    collect(child, &styles, link, runs);
                }
                NodeValue::Link(target) => {
                    collect(child, styles, Some(&target.url), runs);
                }
                // The alt text is the image: the app loads no remote image, and
                // pictures belong to the bikeshed editor.
                NodeValue::Image(_) => {
                    collect(child, styles, None, runs);
                }
                _ => collect(child, styles, link, runs),
            }
        }
    }
}

/// Adjacent runs with the same styles and destination are one run: the parser
/// splits a span around an entity or an escaped character, and the view should not
/// see two runs where the author wrote one.
fn merging_adjacent(runs: Vec<MarkdownInline>) -> Vec<MarkdownInline> {
    let mut merged: Vec<MarkdownInline> = Vec::new();
    for run in runs {
        if let Some(last) = merged.last_mut()
            && last.styles == run.styles
            && last.link == run.link
        {
            last.text.push_str(&run.text);
            continue;
        }
        merged.push(run);
    }
    merged
}

/// The first paragraph in document order, looking inside a list item or a quote.
fn first_paragraph<'a>(node: &'a AstNode<'a>) -> Option<String> {
    for child in node.children() {
        let found = {
            let data = child.data.borrow();
            match &data.value {
                NodeValue::Paragraph => {
                    let text: String = runs(child).into_iter().map(|run| run.text).collect();
                    let text = collapsing_whitespace(&text);
                    (!text.is_empty()).then_some(text)
                }
                NodeValue::BlockQuote
                | NodeValue::List(_)
                | NodeValue::Item(_)
                | NodeValue::TaskItem(_) => first_paragraph(child),
                _ => None,
            }
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn language(info: &str) -> Option<String> {
    let language = info.split_whitespace().next().unwrap_or("");
    if language.is_empty() {
        None
    } else {
        Some(language.to_string())
    }
}

fn collapsing_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn dropping_trailing_newline(text: &str) -> String {
    text.strip_suffix('\n').unwrap_or(text).to_string()
}
