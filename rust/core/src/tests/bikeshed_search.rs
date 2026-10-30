//! Ported from `BikeshedSearchTests`: case-insensitive substring over title and
//! body, one row per note, a title hit wins, a body hit shows a snippet around
//! the first match, sorted by path.

use crate::bikeshed::SearchHit;

use super::bikeshed_support::Vault;

fn hits(vault: &Vault, query: &str) -> Vec<SearchHit> {
    vault
        .bikeshed()
        .search(query)
        .expect("the search runs")
        .value
}

#[test]
fn a_title_hit_shows_the_first_line_in_original_case() {
    let vault = Vault::new("search-title");
    vault.stage("note.md", "My Title\nbody text");
    assert_eq!(
        hits(&vault, "my title"),
        vec![SearchHit {
            path: "note.md".to_string(),
            display: "My Title".to_string(),
        }]
    );
}

#[test]
fn a_title_match_is_case_insensitive() {
    let vault = Vault::new("search-title-case");
    vault.stage("note.md", "My Title\nbody");
    assert_eq!(
        hits(&vault, "MY"),
        vec![SearchHit {
            path: "note.md".to_string(),
            display: "My Title".to_string(),
        }]
    );
}

#[test]
fn a_title_hit_wins_over_a_body_hit() {
    let vault = Vault::new("search-title-wins");
    vault.stage("note.md", "Needle Title\nthe needle in the body");
    assert_eq!(
        hits(&vault, "needle"),
        vec![SearchHit {
            path: "note.md".to_string(),
            display: "Needle Title".to_string(),
        }]
    );
}

#[test]
fn a_body_hit_shows_a_snippet_around_the_first_match() {
    let vault = Vault::new("search-snippet");
    let body = format!("{} NEEDLE {}", "x".repeat(60), "y".repeat(60));
    vault.stage("note.md", &format!("Title line\n{body}"));
    let expected = format!("…{} NEEDLE {}…", "x".repeat(39), "y".repeat(39));
    assert_eq!(
        hits(&vault, "needle"),
        vec![SearchHit {
            path: "note.md".to_string(),
            display: expected,
        }]
    );
}

#[test]
fn a_short_body_shows_no_ellipses() {
    let vault = Vault::new("search-short");
    vault.stage("note.md", "Title\nneedle at start");
    assert_eq!(
        hits(&vault, "needle"),
        vec![SearchHit {
            path: "note.md".to_string(),
            display: "needle at start".to_string(),
        }]
    );
}

#[test]
fn the_snippet_collapses_whitespace_across_lines() {
    let vault = Vault::new("search-collapse");
    vault.stage(
        "note.md",
        "Title\nfirst line\nsecond needle here\nthird line",
    );
    let found = hits(&vault, "needle");
    assert_eq!(found.len(), 1);
    assert!(found[0].display.contains("second needle here"));
    assert!(!found[0].display.contains('\n'));
    assert!(!found[0].display.contains("  "));
}

#[test]
fn multiple_matches_in_one_note_yield_one_row() {
    let vault = Vault::new("search-one-row");
    let body = format!(
        "{} needle one {} needle two {}",
        "x".repeat(60),
        "x".repeat(60),
        "x".repeat(60)
    );
    vault.stage("note.md", &format!("Title\n{body}"));
    let found = hits(&vault, "needle");
    assert_eq!(found.len(), 1);
    assert!(found[0].display.contains("needle one"));
    assert!(!found[0].display.contains("needle two"));
}

#[test]
fn frontmatter_is_searched() {
    let vault = Vault::new("search-frontmatter");
    vault.stage("note.md", "---\ntag: secret-needle\n---\nbody");
    let found = hits(&vault, "secret");
    assert_eq!(found.len(), 1);
    assert!(found[0].display.contains("secret-needle"));
}

#[test]
fn results_are_sorted_by_path_case_insensitively() {
    let vault = Vault::new("search-sorted");
    vault.stage("z/z.md", "needle");
    vault.stage("a/a.md", "needle");
    vault.stage("M.md", "needle");
    assert_eq!(
        hits(&vault, "needle")
            .iter()
            .map(|hit| hit.path.as_str())
            .collect::<Vec<_>>(),
        vec!["a/a.md", "M.md", "z/z.md"]
    );
}

#[test]
fn attachments_and_hidden_files_are_not_searched() {
    let vault = Vault::new("search-scope");
    vault.stage("photo.txt", "needle");
    vault.stage(".hidden.md", "needle");
    assert!(hits(&vault, "needle").is_empty());
}

#[test]
fn no_match_yields_an_empty_result() {
    let vault = Vault::new("search-none");
    vault.stage("note.md", "nothing here");
    assert!(hits(&vault, "needle").is_empty());
}

#[test]
fn an_empty_or_whitespace_query_yields_no_results() {
    let vault = Vault::new("search-empty-query");
    vault.stage("note.md", "anything");
    assert!(hits(&vault, "").is_empty());
    assert!(hits(&vault, "   ").is_empty());
    assert!(hits(&vault, "\n\t").is_empty());
}

#[test]
fn the_query_is_trimmed() {
    let vault = Vault::new("search-trim");
    vault.stage("note.md", "Title\nthe needle here");
    assert_eq!(hits(&vault, "  needle  ").len(), 1);
}

#[test]
fn a_substring_match_is_not_word_boundary_sensitive() {
    let vault = Vault::new("search-substring");
    vault.stage("note.md", "Title\npineapple and needles");
    assert_eq!(hits(&vault, "eap").len(), 1);
    assert_eq!(hits(&vault, "eedl").len(), 1);
}
