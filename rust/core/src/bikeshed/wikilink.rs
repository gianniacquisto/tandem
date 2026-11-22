//! The wikilink model (ADR 0005): the grammar, the exactly-one
//! resolver every reader shares, and the scanner the backlinks, the health
//! report and the rename-rewrite run.
//!
//! The scan is byte-oriented on purpose: a note is read whole, and the tokens are
//! ASCII delimiters, so every index it yields is a `char` boundary of the text.
//! The code-span exemption is `comrak`'s, the same GFM parser the ticket faces
//! read, so a link inside a fence or an inline span is literal text.

use std::collections::HashMap;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

/// A wikilink as written: `[[target[#fragment][|alias]]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wikilink {
    /// The note the link points at, as written (no fragment, no alias).
    pub target: String,
    /// The `#fragment` part, if present (inert in v1).
    pub fragment: Option<String>,
    /// The `|alias` part, if present.
    pub alias: Option<String>,
}

impl Wikilink {
    /// Parse the content between the brackets. `None` for what the grammar says
    /// is not a link: an empty or whitespace-only target, which also covers the
    /// same-note form `[[#Heading]]`.
    pub fn parse(raw: &str) -> Option<Self> {
        let hash = raw.find('#');
        let pipe = raw.find('|');
        let target_end = [hash, pipe]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(raw.len());
        let target = &raw[..target_end];
        if target.trim().is_empty() {
            return None;
        }
        let (fragment, alias) = match (hash, pipe) {
            (Some(hash), Some(pipe)) if hash < pipe => {
                (present(&raw[hash + 1..pipe]), present(&raw[pipe + 1..]))
            }
            (Some(hash), Some(pipe)) => (present(&raw[hash + 1..]), present(&raw[pipe + 1..hash])),
            (Some(hash), None) => (present(&raw[hash + 1..]), None),
            (None, Some(pipe)) => (None, present(&raw[pipe + 1..])),
            (None, None) => (None, None),
        };
        Some(Self {
            target: target.to_string(),
            fragment,
            alias,
        })
    }

    /// The text the editor displays: the alias, else the fragment, else the
    /// target as written.
    pub fn display_text(&self) -> &str {
        self.alias
            .as_deref()
            .or(self.fragment.as_deref())
            .unwrap_or(&self.target)
    }
}

/// An empty or whitespace-only fragment or alias counts as absent.
fn present(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Where a wikilink target resolves, computed live from the file set and the
/// board's names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkResolution {
    /// Exactly one note matches; the path is bikeshed-root-relative.
    Resolved(String),
    /// `[[project:Name]]` named a board project; carries its canonical name.
    Project(String),
    /// `[[label:Name]]` named a board label; carries its canonical name.
    Label(String),
    /// Zero or multiple matches: a broken link.
    Broken,
}

/// The board's names as a plain value (ADR 0054): the projects and each
/// project's labels. The bikeshed never opens the store; the one join point
/// builds this from the board and passes it in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardNames {
    pub projects: Vec<BoardProject>,
}

/// One project and its labels, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardProject {
    pub name: String,
    pub labels: Vec<String>,
}

impl BoardNames {
    /// The project a name matches, case-insensitively, as the board spells it.
    pub fn project(&self, name: &str) -> Option<&BoardProject> {
        self.projects
            .iter()
            .find(|project| project.name.to_lowercase() == name.to_lowercase())
    }

    /// The label a name matches, case-insensitively, as the board spells it
    /// (label names are unique board-wide).
    pub fn label(&self, name: &str) -> Option<&str> {
        let lowered = name.to_lowercase();
        self.projects
            .iter()
            .flat_map(|project| project.labels.iter())
            .find(|label| label.to_lowercase() == lowered)
            .map(String::as_str)
    }
}

/// The typed form of a link target: `project:` or `label:` (case-insensitive
/// prefix) and the name after it. Any other `a:b` is a plain note target.
pub(crate) fn typed_target(target: &str) -> Option<(bool, &str)> {
    let (prefix, name) = target.split_once(':')?;
    match prefix.trim().to_lowercase().as_str() {
        "project" => Some((true, name.trim())),
        "label" => Some((false, name.trim())),
        _ => None,
    }
}

/// The two kinds of board name a typed link can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConceptKind {
    Project,
    Label,
}

impl ConceptKind {
    /// The kind a query value names (`project` or `label`, any case).
    pub fn parse(word: &str) -> Option<Self> {
        match word.trim().to_lowercase().as_str() {
            "project" => Some(Self::Project),
            "label" => Some(Self::Label),
            _ => None,
        }
    }

    /// The word a typed link and a chip use for the kind.
    pub fn word(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Label => "label",
        }
    }
}

/// One wikilink occurrence in a note's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkOccurrence {
    /// The byte offset of the `[[` in the text.
    pub start: usize,
    /// The byte offset just past the closing `]]`.
    pub end: usize,
    pub link: Wikilink,
    pub resolution: LinkResolution,
    /// The line of the text containing the token, without its newline.
    pub line: String,
}

/// A note set pre-indexed for resolution: bare targets on the lowercased
/// basename, qualified targets on the lowercased path, `.md` off.
pub(crate) struct NoteIndex {
    bare: HashMap<String, Vec<String>>,
    qualified: HashMap<String, Vec<String>>,
}

impl NoteIndex {
    pub(crate) fn new(notes: &[String]) -> Self {
        let mut bare: HashMap<String, Vec<String>> = HashMap::new();
        let mut qualified: HashMap<String, Vec<String>> = HashMap::new();
        for note in notes {
            let lower = note.to_lowercase();
            let base = lower.rsplit('/').next().unwrap_or(&lower);
            let base = base.strip_suffix(".md").unwrap_or(base);
            bare.entry(base.to_string()).or_default().push(note.clone());
            let path = lower.strip_suffix(".md").unwrap_or(&lower);
            qualified
                .entry(path.to_string())
                .or_default()
                .push(note.clone());
        }
        Self { bare, qualified }
    }

    /// The shared resolution rule: exactly one match, case-insensitive, with or
    /// without a trailing `.md` on the target. Zero or several is broken.
    ///
    /// A typed target (`project:` or `label:`) resolves first, against the
    /// board's names; every other target is a note target.
    pub(crate) fn resolve(&self, target: &str, names: &BoardNames) -> LinkResolution {
        if let Some((is_project, name)) = typed_target(target) {
            return if is_project {
                names
                    .project(name)
                    .map(|project| LinkResolution::Project(project.name.clone()))
            } else {
                names
                    .label(name)
                    .map(|label| LinkResolution::Label(label.to_string()))
            }
            .unwrap_or(LinkResolution::Broken);
        }
        let lowered = target.to_lowercase();
        let lowered = lowered.strip_suffix(".md").unwrap_or(&lowered);
        let matches = if target.contains('/') {
            self.qualified.get(lowered)
        } else {
            self.bare.get(lowered)
        };
        match matches {
            Some(found) if found.len() == 1 => LinkResolution::Resolved(found[0].clone()),
            _ => LinkResolution::Broken,
        }
    }
}

/// The shortest target that resolves back to each note: the bare basename
/// without `.md` when no other note shares it (case-insensitive, the rule
/// [`NoteIndex::resolve`] applies), else the root-relative path without `.md`.
/// Pairs of `(note path, target)` in the order of `notes`. A root-level note
/// that shares its basename has no resolving target (a target without `/` is
/// bare, and bare is ambiguous there); it gets its path, which is as close as
/// the grammar allows.
pub(crate) fn shortest_targets(notes: &[String]) -> Vec<(String, String)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for note in notes {
        *counts.entry(base_key(note)).or_default() += 1;
    }
    notes
        .iter()
        .map(|note| {
            let path = note.strip_suffix(".md").unwrap_or(note);
            let target = if counts.get(&base_key(note)) == Some(&1) {
                path.rsplit('/').next().unwrap_or(path)
            } else {
                path
            };
            (note.clone(), target.to_string())
        })
        .collect()
}

/// The lowercased basename without `.md`: the key the bare rule matches on.
fn base_key(note: &str) -> String {
    let lower = note.to_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    base.strip_suffix(".md").unwrap_or(base).to_string()
}

/// Every wikilink occurrence in `text`, resolved against `notes` and `names`.
pub(crate) fn link_occurrences(
    text: &str,
    notes: &[String],
    names: &BoardNames,
) -> Vec<LinkOccurrence> {
    let tokens = wikilink_tokens(text);
    if tokens.is_empty() {
        return Vec::new();
    }
    let code = code_ranges(text);
    let index = NoteIndex::new(notes);
    let mut occurrences = Vec::new();
    for (start, end, content) in tokens {
        let Some(link) = Wikilink::parse(content) else {
            continue;
        };
        if code
            .iter()
            .any(|(code_start, code_end)| *code_start < end && start < *code_end)
        {
            continue;
        }
        occurrences.push(LinkOccurrence {
            start,
            end,
            resolution: index.resolve(&link.target, names),
            line: line_containing(text, start).to_string(),
            link,
        });
    }
    occurrences
}

/// The `[[…]]` tokens of `text`, in document order: `(start, end, content)`.
///
/// The first `]]` on the same line closes a `[[`; a newline before the close
/// leaves the `[[` literal; a `[[` immediately preceded by `!` is an embed and
/// therefore literal text.
pub(crate) fn wikilink_tokens(text: &str) -> Vec<(usize, usize, &str)> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        if i + 1 >= bytes.len() || bytes[i + 1] != b'[' {
            i += 1;
            continue;
        }
        if i > 0 && bytes[i - 1] == b'!' {
            i += 2;
            continue;
        }
        let mut close = None;
        let mut j = i + 2;
        while j < bytes.len() {
            if bytes[j] == b'\n' {
                break;
            }
            if bytes[j] == b']' && j + 1 < bytes.len() && bytes[j + 1] == b']' {
                close = Some(j);
                break;
            }
            j += 1;
        }
        match close {
            Some(close) => {
                tokens.push((i, close + 2, &text[i + 2..close]));
                i = close + 2;
            }
            None => i += 2,
        }
    }
    tokens
}

/// The flat byte ranges of every code span in the text: fenced blocks, indented
/// blocks and inline code. Ranges come from the CommonMark parse, converted
/// through the per-line byte starts.
fn code_ranges(text: &str) -> Vec<(usize, usize)> {
    // A document with no code marker cannot carry a code span: no backtick, no
    // fence, no indentation. Skipping the parse there is what keeps a whole-vault
    // scan over prose linear (the scale sentinel).
    if !text.contains('`')
        && !text.contains("~~~")
        && !text.contains("    ")
        && !text.contains('\t')
    {
        return Vec::new();
    }
    let starts = line_start_byte_offsets(text);
    let mut ranges = Vec::new();
    let arena = Arena::new();
    let document = parse_document(&arena, text, &Options::default());
    walk_code(document, starts.as_slice(), text.len(), &mut ranges);
    ranges
}

fn walk_code<'a>(
    node: &'a AstNode<'a>,
    starts: &[usize],
    len: usize,
    ranges: &mut Vec<(usize, usize)>,
) {
    let data = node.data.borrow();
    if matches!(data.value, NodeValue::CodeBlock(_) | NodeValue::Code(_))
        && let Some(range) = flat_range(data.sourcepos, starts, len)
    {
        ranges.push(range);
    }
    drop(data);
    for child in node.children() {
        walk_code(child, starts, len, ranges);
    }
}

/// Convert a source range (1-based line, 1-based byte column; the end inclusive)
/// to a flat byte range, end exclusive.
fn flat_range(
    sourcepos: comrak::nodes::Sourcepos,
    starts: &[usize],
    len: usize,
) -> Option<(usize, usize)> {
    let start_line = sourcepos.start.line.checked_sub(1)?;
    let end_line = sourcepos.end.line.checked_sub(1)?;
    let start = *starts.get(start_line)? + sourcepos.start.column.checked_sub(1)?;
    let end = *starts.get(end_line)? + sourcepos.end.column;
    if start >= end || end > len {
        return None;
    }
    Some((start, end))
}

/// The byte offset of the start of each line of `text` (line 1 at 0).
pub(crate) fn line_start_byte_offsets(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (offset, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(offset + 1);
        }
    }
    starts
}

/// The 1-based line number of a byte position: the count of line starts at or
/// before it.
pub(crate) fn line_number_of(text: &str, index: usize) -> usize {
    let starts = line_start_byte_offsets(text);
    match starts.binary_search(&index) {
        Ok(position) => position + 1,
        Err(position) => position,
    }
}

/// The line of `text` containing `index`, without its newline.
fn line_containing(text: &str, index: usize) -> &str {
    let bytes = text.as_bytes();
    let start = text[..index]
        .rfind('\n')
        .map(|found| found + 1)
        .unwrap_or(0);
    let end = bytes[index..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|found| index + found)
        .unwrap_or(text.len());
    &text[start..end]
}

/// The rename-rewrite for one note's text: replace the target part of every
/// wikilink that resolves to a moved note with its new target, leaving the
/// fragment and alias byte-identical. Tokens inside a leading terminated `---`
/// frontmatter block are skipped. `None` when nothing changed.
pub(crate) fn rewrite_links(
    text: &str,
    notes: &[String],
    mapping: &HashMap<String, String>,
) -> Option<String> {
    let occurrences = link_occurrences(text, notes, &BoardNames::default());
    if occurrences.is_empty() {
        return None;
    }
    let frontmatter = frontmatter_end(text);
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    let mut changed = false;
    for occurrence in &occurrences {
        let LinkResolution::Resolved(path) = &occurrence.resolution else {
            continue;
        };
        let Some(new_target) = mapping.get(path) else {
            continue;
        };
        if frontmatter.is_some_and(|end| occurrence.end <= end) {
            continue;
        }
        let content_start = occurrence.start + 2;
        let close_start = occurrence.end - 2;
        let content = &text[content_start..close_start];
        let target_end = [content.find('#'), content.find('|')]
            .into_iter()
            .flatten()
            .min()
            .map(|offset| content_start + offset)
            .unwrap_or(close_start);
        out.push_str(&text[cursor..content_start]);
        out.push_str(new_target);
        cursor = target_end;
        changed = true;
    }
    if !changed {
        return None;
    }
    out.push_str(&text[cursor..]);
    Some(out)
}

/// The typed rewrite for one note's text (TAN-531): every `[[project:From...]]`
/// (or `[[label:From...]]` when `is_project` is false) becomes `...:To...`, the
/// name matched ignoring case, the prefix as written and the fragment and alias
/// byte-identical. Frontmatter and code spans are skipped, and a plain `[[From]]`
/// is never touched. `None` when nothing changed.
pub(crate) fn rewrite_typed_links(
    text: &str,
    is_project: bool,
    from: &str,
    to: &str,
) -> Option<String> {
    let occurrences = link_occurrences(text, &[], &BoardNames::default());
    let frontmatter = frontmatter_end(text);
    let from = from.trim().to_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    let mut changed = false;
    for occurrence in &occurrences {
        let Some((project, name)) = typed_target(&occurrence.link.target) else {
            continue;
        };
        if project != is_project || name.to_lowercase() != from {
            continue;
        }
        if frontmatter.is_some_and(|end| occurrence.end <= end) {
            continue;
        }
        let content_start = occurrence.start + 2;
        let close_start = occurrence.end - 2;
        let content = &text[content_start..close_start];
        let target_end = [content.find('#'), content.find('|')]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(content.len());
        let target = &content[..target_end];
        let Some(colon) = target.find(':') else {
            continue;
        };
        let after = &target[colon + 1..];
        let leading = after.len() - after.trim_start().len();
        let trailing = after.len() - after.trim_end().len();
        out.push_str(&text[cursor..content_start + colon + 1 + leading]);
        out.push_str(to);
        cursor = content_start + target_end - trailing;
        changed = true;
    }
    if !changed {
        return None;
    }
    out.push_str(&text[cursor..]);
    Some(out)
}

/// The byte offset just past a leading, properly terminated `---` frontmatter
/// block, or `None` (no frontmatter, or unterminated).
pub(crate) fn frontmatter_end(text: &str) -> Option<usize> {
    let first_newline = text.find('\n')?;
    if !is_frontmatter_delimiter(&text[..first_newline]) {
        return None;
    }
    let mut line_start = first_newline + 1;
    while line_start < text.len() {
        match text[line_start..].find('\n') {
            Some(offset) => {
                let newline = line_start + offset;
                if is_frontmatter_delimiter(&text[line_start..newline]) {
                    return Some(newline + 1);
                }
                line_start = newline + 1;
            }
            None => {
                return if is_frontmatter_delimiter(&text[line_start..]) {
                    Some(text.len())
                } else {
                    None
                };
            }
        }
    }
    None
}

fn is_frontmatter_delimiter(line: &str) -> bool {
    line.strip_suffix('\r').unwrap_or(line) == "---"
}
