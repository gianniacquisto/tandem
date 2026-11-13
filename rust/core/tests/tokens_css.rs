//! The design tokens and the stylesheets' colour discipline (CUT-15).
//!
//! Replaces `app/tests/tokens.test.ts` and the colour half of
//! `app/tests/shell.test.ts`: the palette is a fact of the served CSS now, and
//! the check lives where the files do. `tokens.css` owns every colour; a
//! component stylesheet or a template that hard-codes one would drift from the
//! day and night pair this test pins.

const TOKENS: &str = include_str!("../src/serve/static/tokens.css");
const APP: &str = include_str!("../src/serve/static/app.css");
const SHELL: &str = include_str!("../templates/shell.html");
const BOARD: &str = include_str!("../templates/board.html");
const SHEET: &str = include_str!("../templates/sheet.html");
const NOTES: &str = include_str!("../templates/notes.html");
const MARKDOWN: &str = include_str!("../templates/markdown.html");

/// The day palette, in the file's order.
const DAY: [(&str, &str); 20] = [
    ("--paper", "#F1E9D7"),
    ("--parchment", "#E7DECA"),
    ("--sand", "#D6C9AE"),
    ("--shadow", "#C6B897"),
    ("--line", "#C6B99E"),
    ("--ink", "#211D18"),
    ("--ink-dim", "#655D4E"),
    ("--accent", "#6E7D6E"),
    ("--accent-deep", "#55604F"),
    ("--ember", "#A9745C"),
    ("--ember-deep", "#8B5A45"),
    ("--moss", "#6C7A66"),
    ("--moss-deep", "#7E8D77"),
    ("--sky", "#CFDDE2"),
    ("--hill", "#B7C4AE"),
    ("--hill-far", "#C8CFBB"),
    ("--skin", "#E4C9A2"),
    ("--sun", "#E5C07B"),
    ("--wikilink-chip", "#DAD6C5"),
    ("--wikilink-chip-broken", "#EFE3D6"),
];

/// The night palette, in the file's order.
const NIGHT: [(&str, &str); 20] = [
    ("--paper", "#23211C"),
    ("--parchment", "#2C2A23"),
    ("--sand", "#3C3930"),
    ("--shadow", "#191712"),
    ("--line", "#494437"),
    ("--ink", "#EDE6D3"),
    ("--ink-dim", "#9C9583"),
    ("--accent", "#93A48F"),
    ("--accent-deep", "#6E7E6A"),
    ("--ember", "#C08A70"),
    ("--ember-deep", "#9E6B54"),
    ("--moss", "#8FA084"),
    ("--moss-deep", "#74856E"),
    ("--sky", "#2A2E28"),
    ("--hill", "#3A4441"),
    ("--hill-far", "#313A36"),
    ("--skin", "#E3C29B"),
    ("--sun", "#EDE6D3"),
    ("--wikilink-chip", "#33362E"),
    ("--wikilink-chip-broken", "#322D27"),
];

/// The geometry, motion and layout scale, in the file's order.
const SCALE: [&str; 13] = [
    "--hairline",
    "--radius-checkbox",
    "--radius-control",
    "--radius-card",
    "--space-gutter",
    "--space-row",
    "--space-control",
    "--space-lane",
    "--space-chip",
    "--motion-insert",
    "--motion-toggle",
    "--motion-remove",
    "--window-floor",
];

/// The day block is everything before the dark media query, the night block
/// everything after it, so a value cannot be pinned in the wrong scheme.
fn schemes() -> (&'static str, &'static str) {
    TOKENS
        .split_once("@media (prefers-color-scheme: dark)")
        .expect("tokens.css carries the night scheme")
}

#[test]
fn tokens_pin_the_twenty_day_and_night_palette_values() {
    let (day, night) = schemes();
    for (name, value) in DAY {
        let entry = format!("{name}: {value};");
        assert!(day.contains(&entry), "the day palette carries {entry}");
    }
    for (name, value) in NIGHT {
        let entry = format!("{name}: {value};");
        assert!(night.contains(&entry), "the night palette carries {entry}");
    }
}

#[test]
fn tokens_pin_the_geometry_and_motion_scale() {
    for name in SCALE {
        assert!(
            TOKENS.contains(&format!("{name}: ")),
            "tokens.css declares {name}"
        );
    }
}

/// The first raw colour literal a stylesheet holds, if any: a hex value,
/// `rgb()` or `hsl()`. A selector's `#id` is not a colour, and the check reads a
/// hex run only when it is three, six or eight digits long and ends at a
/// boundary.
fn raw_colour(source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'#' {
            let start = index + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_hexdigit() {
                end += 1;
            }
            let digits = end - start;
            if matches!(digits, 3 | 6 | 8) {
                let boundary = bytes
                    .get(end)
                    .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'-');
                if boundary {
                    return Some(source[index..end].to_string());
                }
            }
            index = end;
            continue;
        }
        if bytes[index..].starts_with(b"rgb(") {
            return Some("rgb(".to_string());
        }
        if bytes[index..].starts_with(b"hsl(") {
            return Some("hsl(".to_string());
        }
        index += 1;
    }
    None
}

#[test]
fn component_stylesheets_and_templates_name_no_raw_colour() {
    for (name, source) in [
        ("app.css", APP),
        ("shell.html", SHELL),
        ("board.html", BOARD),
        ("sheet.html", SHEET),
        ("notes.html", NOTES),
        ("markdown.html", MARKDOWN),
    ] {
        assert_eq!(
            raw_colour(source),
            None,
            "{name} hard-codes a colour instead of reading a token"
        );
    }
}
