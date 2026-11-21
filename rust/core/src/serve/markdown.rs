//! Server-rendered markdown: the blocks `TicketMarkdown` parses, drawn as HTML.
//!
//! Askama refuses a recursive macro at compile time, and a markdown document
//! nests blocks inside list items, quotes and task items. The walk lives here,
//! and `markdown.html` wraps what it returns. The class names are the ones the
//! ticket sheet uses, so `app.css` and the notes face keep one dialect.
//!
//! Every text run is escaped on the way out. A ticket body and a note are user
//! text, and the templates never see raw user HTML: the only markup in this
//! output is the renderer's own.

use crate::bikeshed::wikilink::{LinkResolution, Wikilink, wikilink_tokens};
use crate::domain::markdown::{InlineStyle, MarkdownBlock, MarkdownInline};

/// The whole document: the `.markdown` container and every block under it.
pub fn document(blocks: &[MarkdownBlock]) -> String {
    let mut html = String::from("<div class=\"markdown\">");
    blocks_into(blocks, &mut html, None);
    html.push_str("</div>");
    html
}

/// The whole document with its plain `[[note]]` wikilinks drawn as links (the
/// notes read view). The resolver is a value, so this module stays store-free.
pub fn document_with_links(blocks: &[MarkdownBlock], links: &Wikilinks) -> String {
    let mut html = String::from("<div class=\"markdown\">");
    blocks_into(blocks, &mut html, Some(links));
    html.push_str("</div>");
    html
}

/// How a wikilink target resolves and where a resolved note opens.
pub struct Wikilinks<'a> {
    pub resolve: &'a dyn Fn(&str) -> LinkResolution,
    pub href: &'a dyn Fn(&str) -> String,
}

/// The blocks alone, for the `markdown.html` partial's body.
pub fn blocks(blocks: &[MarkdownBlock]) -> String {
    let mut html = String::new();
    blocks_into(blocks, &mut html, None);
    html
}

fn blocks_into(blocks: &[MarkdownBlock], out: &mut String, links: Option<&Wikilinks>) {
    for block in blocks {
        block_into(block, out, links);
    }
}

fn block_into(block: &MarkdownBlock, out: &mut String, links: Option<&Wikilinks>) {
    match block {
        MarkdownBlock::Heading { level, runs } => {
            let tag = format!("h{}", (*level).clamp(1, 6));
            out.push('<');
            out.push_str(&tag);
            out.push_str(" class=\"markdown__heading\">");
            runs_into(runs, out, links);
            out.push_str("</");
            out.push_str(&tag);
            out.push('>');
        }
        MarkdownBlock::Paragraph { runs } => {
            out.push_str("<p class=\"markdown__paragraph\">");
            runs_into(runs, out, links);
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
                blocks_into(item, out, links);
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
            blocks_into(inner, out, links);
            out.push_str("</blockquote>");
        }
        MarkdownBlock::Alert { kind, blocks } => {
            out.push_str("<div class=\"markdown__alert markdown__alert--");
            out.push_str(kind);
            out.push_str("\"><p class=\"markdown__alert-title\">");
            let mut title = kind.to_string();
            title[..1].make_ascii_uppercase();
            out.push_str(&title);
            out.push_str("</p>");
            blocks_into(blocks, out, links);
            out.push_str("</div>");
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
            blocks_into(blocks, out, links);
            out.push_str("</div></div>");
        }
    }
}

fn runs_into(runs: &[MarkdownInline], out: &mut String, links: Option<&Wikilinks>) {
    for run in runs {
        run_into(run, out, links);
    }
}

/// One run, the markers nesting outside in this order:
/// strikethrough outside italic outside bold outside code, with the link's
/// label innermost.
fn run_into(run: &MarkdownInline, out: &mut String, links: Option<&Wikilinks>) {
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
        None => match links {
            Some(links) if !code => text_with_links(&run.text, links, out),
            _ => escape_into(&run.text, out),
        },
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

/// A text run with its plain wikilinks drawn. A typed `[[project:..]]` or
/// `[[label:..]]` stays literal text until its own chips land, and an embed is
/// left to the scanner, which skips it.
fn text_with_links(text: &str, links: &Wikilinks, out: &mut String) {
    let mut last = 0;
    for (start, end, content) in wikilink_tokens(text) {
        let Some(link) = Wikilink::parse(content) else {
            continue;
        };
        let lowered = link.target.trim().to_lowercase();
        if lowered.starts_with("project:") || lowered.starts_with("label:") {
            continue;
        }
        escape_into(&text[last..start], out);
        last = end;
        match (links.resolve)(&link.target) {
            LinkResolution::Resolved(path) => {
                let href = (links.href)(&path);
                out.push_str("<a class=\"wikilink\" href=\"");
                escape_into(&href, out);
                out.push_str("\" hx-get=\"");
                escape_into(&href, out);
                out.push_str("\" hx-target=\"#notes-root\" hx-swap=\"outerHTML\" title=\"");
                escape_into(&path, out);
                out.push_str("\">");
                escape_into(link.display_text(), out);
                out.push_str("</a>");
            }
            LinkResolution::Broken => {
                out.push_str(
                    "<span class=\"wikilink wikilink--broken\" title=\"No single note named ",
                );
                escape_into(&link.target, out);
                out.push_str("\">");
                escape_into(link.display_text(), out);
                out.push_str("</span>");
            }
        }
    }
    escape_into(&text[last..], out);
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

#[cfg(test)]
mod tests {
    use super::document;
    use crate::domain::markdown::TicketMarkdown;

    #[test]
    fn an_alert_renders_as_a_titled_box() {
        let html = document(&TicketMarkdown::blocks(Some("> [!NOTE]\n> read me")));
        assert!(
            html.contains("markdown__alert markdown__alert--note"),
            "{html}"
        );
        assert!(
            html.contains("<p class=\"markdown__alert-title\">Note</p>"),
            "{html}"
        );
        assert!(html.contains("read me"), "{html}");
    }

    #[test]
    fn a_hostile_link_and_raw_markup_never_reach_the_page_as_live_markup() {
        let html = document(&TicketMarkdown::blocks(Some(
            "[x](javascript:alert(1)) <script>alert(2)</script> [d](data:text/html,hi)",
        )));
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<a "), "{html}");
        assert!(!html.contains("href"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }

    #[test]
    fn the_plain_document_draws_no_wikilink_markup() {
        let html = document(&TicketMarkdown::blocks(Some("see [[B]]")));
        assert!(html.contains("[[B]]"), "{html}");
        assert!(!html.contains("wikilink"), "{html}");
    }
}
