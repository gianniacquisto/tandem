//! Ported from `BikeshedHiddenLayerTests`: the capture saddlebag
//! (`projects/<project>/.raw/`) and the consolidation archive
//! (`projects/<project>/.raw/.done/`) are pruned from the tree, the search, the
//! backlinks and the wikilink resolution.
//!
//! The reveal flag changes what the tree draws, never what the curated graph is.

use crate::bikeshed::{Bikeshed, BikeshedNode, NodeKind};

use super::bikeshed_support::Vault;

const CAPTURE: &str = "projects/tandem/.raw/ses_1/20261021T100000Z-work-a-capture.md";
const ARCHIVED: &str = "projects/tandem/.raw/.done/ses_1/20261020T100000Z-work-archived.md";
const CURATED: &str = "projects/tandem/App.md";

fn stage(vault: &Vault) {
    vault.stage(CURATED, "# App\ncurated");
    vault.stage(CAPTURE, "a capture");
    vault.stage(ARCHIVED, "an archived capture");
}

fn find<'a>(nodes: &'a [BikeshedNode], path: &str) -> Option<&'a BikeshedNode> {
    for child in nodes {
        if child.path == path {
            return Some(child);
        }
        if let Some(hit) = find(&child.children, path) {
            return Some(hit);
        }
    }
    None
}

fn note_paths(nodes: &[BikeshedNode]) -> Vec<String> {
    let mut out = Vec::new();
    for node in nodes {
        if node.kind == NodeKind::Note {
            out.push(node.path.clone());
        }
        out.extend(note_paths(&node.children));
    }
    out
}

#[test]
fn the_tree_hides_the_hidden_layer_by_default() {
    let vault = Vault::new("hidden-default");
    stage(&vault);
    let tree = vault.bikeshed().list().unwrap().value;
    assert!(find(&tree, "projects/tandem/.raw").is_none());
    assert_eq!(note_paths(&tree), vec![CURATED.to_string()]);
}

#[test]
fn the_reveal_flag_draws_the_saddlebag_its_sessions_and_the_archive() {
    let vault = Vault::new("hidden-reveal");
    stage(&vault);
    let tree = vault.revealed().list().unwrap().value;
    assert!(find(&tree, "projects/tandem/.raw").is_some());
    assert!(find(&tree, "projects/tandem/.raw/ses_1").is_some());
    assert!(find(&tree, "projects/tandem/.raw/.done").is_some());
    assert!(find(&tree, "projects/tandem/.raw/.done/ses_1").is_some());
    let mut paths = note_paths(&tree);
    paths.sort();
    let mut expected = vec![
        CURATED.to_string(),
        CAPTURE.to_string(),
        ARCHIVED.to_string(),
    ];
    expected.sort();
    assert_eq!(paths, expected);
}

#[test]
fn the_reveal_flag_is_a_view_the_linkable_set_is_unchanged() {
    let vault = Vault::new("hidden-linkable");
    stage(&vault);
    assert_eq!(vault.bikeshed().note_paths(), vec![CURATED.to_string()]);
    assert_eq!(vault.revealed().note_paths(), vec![CURATED.to_string()]);
}

#[test]
fn search_ignores_the_hidden_layer_in_both_states() {
    let vault = Vault::new("hidden-search");
    stage(&vault);
    assert!(vault.bikeshed().search("capture").unwrap().value.is_empty());
    assert!(vault.revealed().search("capture").unwrap().value.is_empty());
}

#[test]
fn resolution_ignores_the_hidden_layer_in_both_states() {
    let vault = Vault::new("hidden-resolve");
    stage(&vault);
    for bikeshed in [vault.bikeshed(), vault.revealed()] {
        assert_eq!(
            bikeshed.resolve("20261021T100000Z-work-a-capture"),
            crate::bikeshed::wikilink::LinkResolution::Broken
        );
        assert_eq!(
            bikeshed.resolve("projects/tandem/.raw/ses_1/20261021T100000Z-work-a-capture"),
            crate::bikeshed::wikilink::LinkResolution::Broken
        );
        assert_eq!(
            bikeshed.resolve("App"),
            crate::bikeshed::wikilink::LinkResolution::Resolved(CURATED.to_string())
        );
    }
}

#[test]
fn backlinks_ignore_the_hidden_layer_in_both_states() {
    let vault = Vault::new("hidden-backlinks");
    stage(&vault);
    vault.stage(CAPTURE, "a capture linking [[App]]");
    for bikeshed in [vault.bikeshed(), vault.revealed()] {
        assert!(bikeshed.backlinks(CURATED).unwrap().value.is_empty());
    }
}

#[test]
fn a_curated_links_to_a_capture_is_broken_in_both_states() {
    let vault = Vault::new("hidden-curated-link");
    stage(&vault);
    vault.stage(CURATED, "# App\nsee [[20261021T100000Z-work-a-capture]]");
    for bikeshed in [vault.bikeshed(), vault.revealed()] {
        assert_eq!(
            bikeshed.resolve("20261021T100000Z-work-a-capture"),
            crate::bikeshed::wikilink::LinkResolution::Broken
        );
        assert!(bikeshed.backlinks(CAPTURE).unwrap().value.is_empty());
    }
}

#[test]
fn a_revealed_capture_is_readable_and_writable() {
    let vault = Vault::new("hidden-write");
    stage(&vault);
    let bikeshed: Bikeshed = vault.revealed();
    assert_eq!(bikeshed.read(CAPTURE).unwrap().value.text, "a capture");
    bikeshed.write(CAPTURE, "edited capture").unwrap();
    assert_eq!(bikeshed.read(CAPTURE).unwrap().value.text, "edited capture");
}

#[test]
fn deleting_a_revealed_capture_removes_the_file() {
    let vault = Vault::new("hidden-delete");
    stage(&vault);
    let bikeshed = vault.revealed();
    bikeshed.delete(CAPTURE).unwrap();
    assert!(!vault.exists(CAPTURE));
    assert!(vault.exists(ARCHIVED));
}

#[test]
fn creating_inside_a_hidden_folder_follows_the_flag() {
    let vault = Vault::new("hidden-create");
    stage(&vault);
    let target = "projects/tandem/.raw/ses_1/new.md";
    assert_eq!(
        vault.bikeshed().create(target).unwrap_err(),
        crate::domain::error::DomainError::InvalidName(target.to_string())
    );
    vault.revealed().create(target).unwrap();
    assert!(vault.exists(target));
}

#[test]
fn a_captures_write_path_stays_valid() {
    let vault = Vault::new("hidden-capture");
    let bikeshed = vault.revealed();
    let path = bikeshed
        .capture(
            "tandem",
            crate::bikeshed::CaptureKind::Fact,
            "a capture",
            "ses_1",
            super::bikeshed_support::instant(super::bikeshed_support::INSTANT),
        )
        .unwrap()
        .value;
    assert!(path.starts_with("projects/tandem/.raw/ses_1/"));
    assert!(vault.exists(&path));
}
