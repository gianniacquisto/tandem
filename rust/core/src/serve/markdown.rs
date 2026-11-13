//! Server-rendered markdown: the blocks `TicketMarkdown` parses, drawn as HTML.
//!
//! Askama refuses a recursive macro at compile time, and a markdown document
//! nests blocks inside list items, quotes and task items. The walk lives here,
//! and `markdown.html` wraps what it returns. The class names are the ones the
//! React renderer drew, so `app.css` and the notes face keep one dialect.
//!
//! Every text run is escaped on the way out. A ticket body and a note are user
//! text, and the templates never see raw user HTML: the only markup in this
//! output is the renderer's own.

use crate::domain::markdown::{InlineStyle, MarkdownBlock, MarkdownInline};

/// The whole document: the `.markdown` container and every block under it.
pub fn document(blocks: &[MarkdownBlock]) -> String {
    let mut html = String::from("<div class=\"markdown\">");
    blocks_into(blocks, &mut html);
    html.push_str("</div>");
    html
}

/// The blocks alone, for the `markdown.html` partial's body.
pub fn blocks(blocks: &[MarkdownBlock]) -> String {
    let mut html = String::new();
    blocks_into(blocks, &mut html);
    html
}

fn blocks_into(blocks: &[MarkdownBlock], out: &mut String) {
    for block in blocks {
        block_into(block, out);
    }
}

fn block_into(block: &MarkdownBlock, out: &mut String) {
    match block {
        MarkdownBlock::Heading { level, runs } => {
            let tag = format!("h{}", (*level).clamp(1, 6));
            out.push('<');
            out.push_str(&tag);
            out.push_str(" class=\"markdown__heading\">");
            runs_into(runs, out);
            out.push_str("</");
            out.push_str(&tag);
            out.push('>');
        }
        MarkdownBlock::Paragraph { runs } => {
            out.push_str("<p class=\"markdown__paragraph\">");
            runs_into(runs, out);
            out.push_str("</p>");
        }
        MarkdownBlock::List {
            ordered,
            start,
            items,
        } => {
            let tag = if *ordered { "ol" } else { "ul" };
            out.push('<');
            out.push_str(tag);
            out.push_str(" class=\"markdown__list\"");
            if *ordered && *start != 1 {
                out.push_str(" start=\"");
                out.push_str(&start.to_string());
                out.push('"');
            }
            out.push('>');
            for item in items {
                out.push_str("<li class=\"markdown__item\">");
                blocks_into(item, out);
                out.push_str("</li>");
            }
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        MarkdownBlock::Code { language, text } => {
            out.push_str("<pre class=\"markdown__code\"");
            if let Some(language) = language {
                out.push_str(" data-language=\"");
                escape_into(language, out);
                out.push('"');
            }
            out.push_str("><code>");
            escape_into(text, out);
            out.push_str("</code></pre>");
        }
        MarkdownBlock::Quote(inner) => {
            out.push_str("<blockquote class=\"markdown__quote\">");
            blocks_into(inner, out);
            out.push_str("</blockquote>");
        }
        MarkdownBlock::Table { header, rows } => {
            out.push_str("<table class=\"markdown__table\">");
            if !header.is_empty() {
                out.push_str("<thead><tr>");
                for cell in header {
                    out.push_str("<th>");
                    escape_into(cell, out);
                    out.push_str("</th>");
                }
                out.push_str("</tr></thead>");
            }
            out.push_str("<tbody>");
            for row in rows {
                out.push_str("<tr>");
                for cell in row {
                    out.push_str("<td>");
                    escape_into(cell, out);
                    out.push_str("</td>");
                }
                out.push_str("</tr>");
            }
            out.push_str("</tbody></table>");
        }
        MarkdownBlock::Rule => out.push_str("<hr class=\"markdown__rule\">"),
        MarkdownBlock::Task { checked, blocks } => {
            out.push_str("<div class=\"markdown__task\"");
            if *checked {
                out.push_str(" data-checked=\"true\"");
            }
            out.push_str("><span class=\"markdown__checkbox\" role=\"checkbox\" aria-checked=\"");
            out.push_str(if *checked { "true" } else { "false" });
            out.push_str("\"></span><div class=\"markdown__task-body\">");
            blocks_into(blocks, out);
            out.push_str("</div></div>");
        }
    }
}

fn runs_into(runs: &[MarkdownInline], out: &mut String) {
    for run in runs {
        run_into(run, out);
    }
}

/// One run, the markers nesting as the React renderer nested them:
/// strikethrough outside italic outside bold outside code, with the link's
/// label innermost.
fn run_into(run: &MarkdownInline, out: &mut String) {
    let strike = run.has(InlineStyle::Strikethrough);
    let italic = run.has(InlineStyle::Italic);
    let bold = run.has(InlineStyle::Bold);
    let code = run.has(InlineStyle::Code);
    if strike {
        out.push_str("<s>");
    }
    if italic {
        out.push_str("<em>");
    }
    if bold {
        out.push_str("<strong>");
    }
    if code {
        out.push_str("<code class=\"markdown__inline-code\">");
    }
    match &run.link {
        Some(link) => {
            out.push_str("<span class=\"markdown__link\" title=\"");
            escape_into(link, out);
            out.push_str("\">");
            escape_into(&run.text, out);
            out.push_str("</span>");
        }
        None => escape_into(&run.text, out),
    }
    if code {
        out.push_str("</code>");
    }
    if bold {
        out.push_str("</strong>");
    }
    if italic {
        out.push_str("</em>");
    }
    if strike {
        out.push_str("</s>");
    }
}

/// The five characters that would change the markup, escaped.
fn escape_into(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
}
