//! Ported from `BikeshedScaleTests`: one generous, non-flaky bound. A full
//! bikeshed-wide recompute (every note read, scanned, and its wikilinks
//! resolved, the heaviest derived operation: `backlinks` over a hub note) over a
//! synthetic vault of hundreds of notes and ~100 MB completes in seconds.
//!
//! The bound catches order-of-magnitude regressions, not scheduler noise.

use std::time::Instant;

use super::bikeshed_support::Vault;

/// ~0.34 MB of deterministic prose: a heading, bulk paragraphs, and a realistic
/// link load (~250 wikilinks per note, to the hub and a few siblings) so the scan
/// and resolver do real work on every note.
fn synthetic_note(index: usize) -> String {
    let mut out = format!("# Note {index}\n\n");
    out.reserve(360_000);
    for paragraph in 0..2_500 {
        out.push_str(&format!(
            "Paragraph {paragraph} of note {index}: the quick brown fox jumps over the lazy dog. "
        ));
        out.push_str("Bulk text to keep the note at a realistic size for the sentinel. ");
        if paragraph % 20 == 0 {
            out.push_str(&format!(
                "It links to [[Hub]] and to [[Note {}]]. ",
                index % 50
            ));
        }
        out.push_str("\n\n");
    }
    out
}

#[test]
fn a_full_recompute_over_about_100_mb_of_hundreds_of_notes_stays_fast() {
    let vault = Vault::new("scale");
    let bikeshed = vault.bikeshed();

    let note_count = 280;
    let folders = 14;
    let mut staged = 0usize;
    for index in 0..note_count {
        let folder = format!("folder-{}", index / (note_count / folders));
        let text = synthetic_note(index);
        let path = format!("{folder}/note-{index}.md");
        vault.stage(&path, &text);
        staged += text.len();
    }
    vault.stage("Hub.md", "The hub note.\n");
    let total_bytes = staged + 14;
    let expected = note_count;

    let start = Instant::now();
    let rows = bikeshed
        .backlinks("Hub.md")
        .expect("the recompute runs")
        .value;
    let elapsed = start.elapsed();

    eprintln!(
        "SCALE {} MB, {} notes: {:.2} s",
        total_bytes / (1024 * 1024),
        note_count,
        elapsed.as_secs_f64()
    );
    assert!(
        total_bytes >= 90 * 1024 * 1024,
        "bikeshed smaller than intended: {total_bytes} bytes"
    );
    assert_eq!(rows.len(), expected, "expected {expected} backlink rows");
    assert!(
        elapsed.as_secs() < 45,
        "full recompute took {} s, should stay in seconds",
        elapsed.as_secs()
    );
}
