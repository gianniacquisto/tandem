//! Ported from `BikeshedWikilinkTests`: the grammar (bare, qualified, alias,
//! fragment, the first-`#`-then-first-`|` split, the same-note form), the
//! case-insensitive exactly-one resolution, and the scanner's exemptions.

use crate::bikeshed::wikilink::{LinkResolution, Wikilink};

use super::bikeshed_support::Vault;

// MARK: - Grammar

#[test]
fn parse_bare_target_and_display() {
    let link = Wikilink::parse("Note").expect("a bare link");
    assert_eq!(link.target, "Note");
    assert_eq!(link.fragment, None);
    assert_eq!(link.alias, None);
    assert_eq!(link.display_text(), "Note");
}

#[test]
fn parse_alias_displays_the_alias() {
    let link = Wikilink::parse("Note|Alias").expect("an aliased link");
    assert_eq!(link.target, "Note");
    assert_eq!(link.alias.as_deref(), Some("Alias"));
    assert_eq!(link.display_text(), "Alias");
}

#[test]
fn parse_fragment_displays_the_fragment() {
    let link = Wikilink::parse("Note#Heading").expect("a fragment link");
    assert_eq!(link.target, "Note");
    assert_eq!(link.fragment.as_deref(), Some("Heading"));
    assert_eq!(link.alias, None);
    assert_eq!(link.display_text(), "Heading");
}

#[test]
fn parse_fragment_and_alias_split_at_the_first_hash_then_pipe() {
    let link = Wikilink::parse("Note#Heading|Alias").expect("a full link");
    assert_eq!(link.target, "Note");
    assert_eq!(link.fragment.as_deref(), Some("Heading"));
    assert_eq!(link.alias.as_deref(), Some("Alias"));
    assert_eq!(link.display_text(), "Alias");
}

#[test]
fn parse_later_hash_and_pipe_stay_in_the_fragment_or_alias() {
    let link = Wikilink::parse("a#b|c#d").expect("a link");
    assert_eq!(link.target, "a");
    assert_eq!(link.fragment.as_deref(), Some("b"));
    assert_eq!(link.alias.as_deref(), Some("c#d"));
    assert_eq!(link.display_text(), "c#d");
}

#[test]
fn parse_alias_before_fragment_is_still_parsed() {
    let link = Wikilink::parse("a|b#Heading").expect("a reversed link");
    assert_eq!(link.target, "a");
    assert_eq!(link.alias.as_deref(), Some("b"));
    assert_eq!(link.fragment.as_deref(), Some("Heading"));
    assert_eq!(link.display_text(), "b");
}

#[test]
fn parse_an_empty_or_whitespace_only_target_is_not_a_link() {
    assert!(Wikilink::parse("").is_none());
    assert!(Wikilink::parse("   ").is_none());
    assert!(Wikilink::parse("\t").is_none());
}

#[test]
fn parse_the_same_note_form_is_not_a_link() {
    assert!(Wikilink::parse("#Heading").is_none());
    assert!(Wikilink::parse("#").is_none());
}

#[test]
fn parse_empty_edge_cases() {
    assert!(Wikilink::parse("|alias").is_none());
    let link = Wikilink::parse("a|").expect("a link");
    assert_eq!(link.target, "a");
    assert_eq!(link.alias, None);
    assert_eq!(link.display_text(), "a");
}

// MARK: - Resolution

#[test]
fn resolve_bare_matches_the_basename_case_insensitively() {
    let vault = Vault::new("wikilink-bare");
    vault.stage("Note.md", "x");
    let bikeshed = vault.bikeshed();

    assert_eq!(
        bikeshed.resolve("note"),
        LinkResolution::Resolved("Note.md".to_string())
    );
    assert_eq!(
        bikeshed.resolve("NOTE"),
        LinkResolution::Resolved("Note.md".to_string())
    );
    assert_eq!(
        bikeshed.resolve("note.md"),
        LinkResolution::Resolved("Note.md".to_string())
    );
}

#[test]
fn resolve_zero_matches_is_broken() {
    let vault = Vault::new("wikilink-zero");
    vault.stage("a.md", "x");
    let bikeshed = vault.bikeshed();

    assert_eq!(bikeshed.resolve("missing"), LinkResolution::Broken);
    assert_eq!(bikeshed.resolve("a/x"), LinkResolution::Broken);
}

#[test]
fn resolve_a_unique_bare_target_in_a_subfolder() {
    let vault = Vault::new("wikilink-subfolder");
    vault.stage("a/x.md", "x");
    assert_eq!(
        vault.bikeshed().resolve("x"),
        LinkResolution::Resolved("a/x.md".to_string())
    );
}

#[test]
fn resolve_duplicate_basenames_are_broken_never_a_silent_match() {
    let vault = Vault::new("wikilink-duplicates");
    vault.stage("a/x.md", "1");
    vault.stage("b/x.md", "2");
    vault.stage("x.md", "3");
    let bikeshed = vault.bikeshed();

    assert_eq!(bikeshed.resolve("x"), LinkResolution::Broken);
    assert_eq!(bikeshed.resolve("X"), LinkResolution::Broken);
    assert_eq!(
        bikeshed.resolve("b/x"),
        LinkResolution::Resolved("b/x.md".to_string())
    );
}

#[test]
fn resolve_qualified_targets_are_paths_case_insensitively() {
    let vault = Vault::new("wikilink-qualified");
    vault.stage("projects/plan.md", "p");
    let bikeshed = vault.bikeshed();

    assert_eq!(
        bikeshed.resolve("projects/plan"),
        LinkResolution::Resolved("projects/plan.md".to_string())
    );
    assert_eq!(
        bikeshed.resolve("Projects/PLAN"),
        LinkResolution::Resolved("projects/plan.md".to_string())
    );
    assert_eq!(
        bikeshed.resolve("projects/plan.md"),
        LinkResolution::Resolved("projects/plan.md".to_string())
    );
    assert_eq!(bikeshed.resolve("other/plan"), LinkResolution::Broken);
    assert_eq!(
        bikeshed.resolve("projects/deep/plan"),
        LinkResolution::Broken
    );
}

#[test]
fn resolve_attachments_are_never_linkable() {
    let vault = Vault::new("wikilink-attachments");
    vault.stage("photo.png", "img");
    vault.stage("a/photo.png", "img2");
    let bikeshed = vault.bikeshed();

    assert_eq!(bikeshed.resolve("photo"), LinkResolution::Broken);
    assert_eq!(bikeshed.resolve("photo.png"), LinkResolution::Broken);
    assert_eq!(bikeshed.resolve("a/photo"), LinkResolution::Broken);
}

// MARK: - Scanning

#[test]
fn scan_a_wikilink_outside_code_carries_range_form_and_resolution() {
    let vault = Vault::new("wikilink-scan");
    vault.stage("Note.md", "x");
    let bikeshed = vault.bikeshed();

    let text = "see [[Note]] here";
    let found = bikeshed.links(text);
    assert_eq!(found.len(), 1);
    let occurrence = &found[0];
    assert_eq!(&text[occurrence.start..occurrence.end], "[[Note]]");
    assert_eq!(occurrence.link.target, "Note");
    assert_eq!(
        occurrence.resolution,
        LinkResolution::Resolved("Note.md".to_string())
    );
    assert_eq!(occurrence.line, text);
}

#[test]
fn scan_broken_links_are_reported() {
    let vault = Vault::new("wikilink-broken");
    let found = vault.bikeshed().links("[[Missing]]");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].resolution, LinkResolution::Broken);
}

#[test]
fn scan_fenced_code_blocks_are_literal_text() {
    let vault = Vault::new("wikilink-fenced");
    vault.stage("Note.md", "x");
    let bikeshed = vault.bikeshed();

    let backticks = "before\n```\n[[Note]]\n```\nafter [[Note]]";
    let tildes = "~~~swift\n[[Note]]\n~~~";
    assert_eq!(bikeshed.links(backticks).len(), 1, "only the real link");
    assert!(bikeshed.links(tildes).is_empty());
}

#[test]
fn scan_inline_code_is_literal_text() {
    let vault = Vault::new("wikilink-inline");
    vault.stage("Note.md", "x");
    let bikeshed = vault.bikeshed();

    let text = "use `[[Note]]` not [[Note]]";
    let found = bikeshed.links(text);
    assert_eq!(found.len(), 1);
    assert_eq!(&text[found[0].start..found[0].end], "[[Note]]");
}

#[test]
fn scan_embeds_are_literal_text() {
    let vault = Vault::new("wikilink-embeds");
    vault.stage("Note.md", "x");
    let bikeshed = vault.bikeshed();

    assert!(bikeshed.links("![[Note]]").is_empty());
    assert_eq!(bikeshed.links("x ![[Note]] and [[Note]]").len(), 1);
}

#[test]
fn scan_an_unclosed_open_is_literal_text() {
    let vault = Vault::new("wikilink-unclosed");
    vault.stage("Note.md", "x");
    let bikeshed = vault.bikeshed();

    assert_eq!(bikeshed.links("a [[Note\nb [[Note]]").len(), 1);
    assert!(bikeshed.links("x [[Note").is_empty());
    assert!(bikeshed.links("[[A\nB]]").is_empty());
}

#[test]
fn scan_the_first_close_wins() {
    let vault = Vault::new("wikilink-first-close");
    vault.stage("a.md", "x");
    let found = vault.bikeshed().links("[[a]]b]] [[a]]");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].link.target, "a");
}

#[test]
fn scan_links_in_blockquotes_and_list_items_are_links() {
    let vault = Vault::new("wikilink-blocks");
    vault.stage("Note.md", "x");
    let found = vault.bikeshed().links("> quote [[Note]]\n- item [[Note]]");
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].line, "> quote [[Note]]");
    assert_eq!(found[1].line, "- item [[Note]]");
}

#[test]
fn scan_indented_code_blocks_are_literal_text() {
    let vault = Vault::new("wikilink-indented");
    vault.stage("Note.md", "x");
    assert_eq!(
        vault.bikeshed().links("    [[Note]]\nreal [[Note]]").len(),
        1
    );
}
