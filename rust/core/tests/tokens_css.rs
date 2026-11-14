//! The design tokens and the stylesheets' colour discipline (CUT-15).
//!
//! Replaces `app/tests/tokens.test.ts` and the colour half of
//! `app/tests/shell.test.ts`: the palette is a fact of the served CSS now, and
//! the check lives where the files do. `tokens.css` owns every colour; a
//! component stylesheet or a template that hard-codes one would drift from the
//! four palettes this test pins.

const TOKENS: &str = include_str!("../src/serve/static/tokens.css");
const APP: &str = include_str!("../src/serve/static/app.css");
const SHELL: &str = include_str!("../templates/shell.html");
const BOARD: &str = include_str!("../templates/board.html");
const SHEET: &str = include_str!("../templates/sheet.html");
const NOTES: &str = include_str!("../templates/notes.html");
const MARKDOWN: &str = include_str!("../templates/markdown.html");

/// The four palettes, in the file's order, keyed by the selector that scopes them.
const NEUTRAL_LIGHT: [(&str, &str); 34] = [
    ("--paper", "#FFFFFF"),
    ("--parchment", "#F6F5F4"),
    ("--sand", "#EDE9E4"),
    ("--shadow", "#C8C4BE"),
    ("--line", "#DDD9D3"),
    ("--ink", "#1A1A1A"),
    ("--ink-dim", "#5D5B54"),
    ("--accent", "#5645D4"),
    ("--accent-deep", "#4534B3"),
    ("--ember", "#9A3D00"),
    ("--ember-deep", "#793400"),
    ("--moss", "#1B7F7B"),
    ("--moss-deep", "#146560"),
    ("--sky", "#DCECFA"),
    ("--hill", "#D9F3E1"),
    ("--hill-far", "#E6E0F5"),
    ("--skin", "#FFE8D4"),
    ("--sun", "#F5D75E"),
    ("--wikilink-chip", "#E6E0F5"),
    ("--wikilink-chip-broken", "#FDE0EC"),
    ("--card", "#FFFFFF"),
    ("--tint-backlog", "#F0EEEC"),
    ("--tint-on-hold", "#FEF7D6"),
    ("--tint-ready", "#DCECFA"),
    ("--tint-in-progress", "#FFE8D4"),
    ("--tint-in-review", "#E6E0F5"),
    ("--tint-done", "#D9F3E1"),
    ("--tone-moss", "#2A9D99"),
    ("--tone-ember", "#DD5B00"),
    ("--tone-accent", "#5645D4"),
    ("--tone-sun", "#A67C00"),
    ("--tone-skin", "#A8683A"),
    ("--tone-hill", "#168F2E"),
    ("--tone-sky", "#0075DE"),
];

const NEUTRAL_DARK: [(&str, &str); 34] = [
    ("--paper", "#191918"),
    ("--parchment", "#222120"),
    ("--sand", "#2D2B29"),
    ("--shadow", "#0E0E0D"),
    ("--line", "#3A3835"),
    ("--ink", "#EDEBE6"),
    ("--ink-dim", "#A9A59C"),
    ("--accent", "#A89DF2"),
    ("--accent-deep", "#CFC8FA"),
    ("--ember", "#F0A070"),
    ("--ember-deep", "#F4B08A"),
    ("--moss", "#5CC4BF"),
    ("--moss-deep", "#7AD3CE"),
    ("--sky", "#1F2D3A"),
    ("--hill", "#1D2F25"),
    ("--hill-far", "#2B2640"),
    ("--skin", "#3A2B20"),
    ("--sun", "#EDE6D3"),
    ("--wikilink-chip", "#2E2A44"),
    ("--wikilink-chip-broken", "#43283A"),
    ("--card", "#2A2927"),
    ("--tint-backlog", "#212120"),
    ("--tint-on-hold", "#2B2818"),
    ("--tint-ready", "#1D2A38"),
    ("--tint-in-progress", "#32261C"),
    ("--tint-in-review", "#27233A"),
    ("--tint-done", "#1C2B22"),
    ("--tone-moss", "#5CC4BF"),
    ("--tone-ember", "#F08A4B"),
    ("--tone-accent", "#A89DF2"),
    ("--tone-sun", "#E5C455"),
    ("--tone-skin", "#D49A6A"),
    ("--tone-hill", "#5CCB78"),
    ("--tone-sky", "#5AAEF0"),
];

const TANDEM_LIGHT: [(&str, &str); 34] = [
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
    ("--card", "#F6F0E2"),
    ("--tint-backlog", "#EAE2CF"),
    ("--tint-on-hold", "#EFE3C4"),
    ("--tint-ready", "#DEE5E0"),
    ("--tint-in-progress", "#EBDCC4"),
    ("--tint-in-review", "#E0E3D8"),
    ("--tint-done", "#DCE4D2"),
    ("--tone-moss", "#6C7A66"),
    ("--tone-ember", "#A9745C"),
    ("--tone-accent", "#6E7D6E"),
    ("--tone-sun", "#9A7420"),
    ("--tone-skin", "#A07A45"),
    ("--tone-hill", "#5F7A52"),
    ("--tone-sky", "#5E8696"),
];

const TANDEM_DARK: [(&str, &str); 34] = [
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
    ("--card", "#2C2A23"),
    ("--tint-backlog", "#27251F"),
    ("--tint-on-hold", "#2F2B1F"),
    ("--tint-ready", "#262B27"),
    ("--tint-in-progress", "#2F2922"),
    ("--tint-in-review", "#282A24"),
    ("--tint-done", "#252B23"),
    ("--tone-moss", "#8FA084"),
    ("--tone-ember", "#C08A70"),
    ("--tone-accent", "#93A48F"),
    ("--tone-sun", "#E0BC70"),
    ("--tone-skin", "#E3C29B"),
    ("--tone-hill", "#8AA67C"),
    ("--tone-sky", "#8FB3C2"),
];

/// The geometry, motion and layout scale, in the file's order.
const SCALE: [&str; 15] = [
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
    "--font-body",
    "--font-mono",
];

/// The declaration block a palette's selector opens.
fn palette(selector: &str) -> &'static str {
    let start = TOKENS
        .find(&format!("{selector} {{"))
        .unwrap_or_else(|| panic!("tokens.css carries {selector}"));
    let rest = &TOKENS[start..];
    let open = rest.find('{').unwrap() + 1;
    let close = rest.find('}').unwrap();
    &rest[open..close]
}

const SELECTORS: [(&str, &str); 4] = [
    (
        "neutral light",
        r#"html[data-style="neutral"][data-theme="light"]"#,
    ),
    (
        "neutral dark",
        r#"html[data-style="neutral"][data-theme="dark"]"#,
    ),
    (
        "tandem light",
        r#"html[data-style="tandem"][data-theme="light"]"#,
    ),
    (
        "tandem dark",
        r#"html[data-style="tandem"][data-theme="dark"]"#,
    ),
];

fn pins(label: &str, selector: &str, expected: &[(&str, &str)]) {
    let block = palette(selector);
    for (name, value) in expected {
        let entry = format!("{name}: {value};");
        assert!(block.contains(&entry), "{label} carries {entry}");
    }
    let declared = block
        .lines()
        .filter(|l| l.trim_start().starts_with("--"))
        .count();
    assert_eq!(
        declared,
        expected.len(),
        "{label} declares only its pinned tokens"
    );
}

#[test]
fn tokens_pin_every_value_in_all_four_palettes() {
    pins("neutral light", SELECTORS[0].1, &NEUTRAL_LIGHT);
    pins("neutral dark", SELECTORS[1].1, &NEUTRAL_DARK);
    pins("tandem light", SELECTORS[2].1, &TANDEM_LIGHT);
    pins("tandem dark", SELECTORS[3].1, &TANDEM_DARK);
}

#[test]
fn the_four_palettes_declare_the_same_token_names() {
    for set in [&NEUTRAL_DARK[..], &TANDEM_LIGHT[..], &TANDEM_DARK[..]] {
        let names: Vec<_> = set.iter().map(|(n, _)| *n).collect();
        let first: Vec<_> = NEUTRAL_LIGHT.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, first);
    }
}

#[test]
fn neutral_light_is_the_default_before_a_theme_attribute_is_set() {
    // A checkout may carry the file with CRLF line ends.
    let tokens = TOKENS.replace("\r\n", "\n");
    assert!(tokens.contains(":root,\nhtml[data-style=\"neutral\"][data-theme=\"light\"] {"));
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

fn channel(hex: &str, at: usize) -> f64 {
    let v = f64::from(u8::from_str_radix(&hex[at..at + 2], 16).unwrap()) / 255.0;
    if v <= 0.03928 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(hex: &str) -> f64 {
    0.2126 * channel(hex, 1) + 0.7152 * channel(hex, 3) + 0.0722 * channel(hex, 5)
}

fn contrast(a: &str, b: &str) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn value<'a>(set: &'a [(&'a str, &'a str)], name: &str) -> &'a str {
    set.iter().find(|(n, _)| *n == name).unwrap().1
}

/// AA for the neutral palettes: text is 4.5:1 on every surface, tinted column
/// and card it can sit on, and a lane dot is 3:1 on the card. The tandem
/// palettes are the frozen cutover values and are not held to this bar.
#[test]
fn neutral_palettes_meet_aa_on_surfaces_columns_and_cards() {
    for (label, set) in [
        ("neutral light", &NEUTRAL_LIGHT[..]),
        ("neutral dark", &NEUTRAL_DARK[..]),
    ] {
        let surfaces: Vec<&str> = set
            .iter()
            .filter(|(n, _)| {
                matches!(*n, "--paper" | "--parchment" | "--card") || n.starts_with("--tint-")
            })
            .map(|(n, _)| *n)
            .collect();
        for text in [
            "--ink",
            "--ink-dim",
            "--accent",
            "--accent-deep",
            "--ember",
            "--ember-deep",
            "--moss-deep",
        ] {
            for surface in &surfaces {
                let ratio = contrast(value(set, text), value(set, surface));
                assert!(ratio >= 4.5, "{label}: {text} on {surface} is {ratio:.2}");
            }
        }
        for (name, dot) in set.iter().filter(|(n, _)| n.starts_with("--tone-")) {
            let ratio = contrast(dot, value(set, "--card"));
            assert!(ratio >= 3.0, "{label}: {name} on the card is {ratio:.2}");
        }
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
