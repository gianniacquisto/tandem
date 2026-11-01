//! Ported from `ScaffoldConventionTests`: the conventions state the fixed
//! scaffold, the input rule, and the daily notes' read-only rule. The repo's
//! rules live in the skill files, not in the app, so this suite pins the
//! load-bearing phrases against a refactor dropping the contract silently.
//!
//! The files are read from the repository root, not from a fixture.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root resolves")
}

fn file(path: &str) -> String {
    std::fs::read_to_string(root().join(path)).unwrap_or_else(|_| panic!("missing {path}"))
}

/// The retired orient wordings: the scaffold's anchor is the Overview, never the
/// Hub or a "curated note". Both apostrophes fold to the ASCII one.
fn names_a_stale_orient_phrase(text: &str) -> bool {
    let folded = text.replace(['\u{2018}', '\u{2019}'], "'");
    ["the project's curated note", "orient from the Hub"]
        .iter()
        .any(|phrase| folded.contains(phrase))
}

/// The text after `heading`, up to the next `## ` heading.
fn section(text: &str, heading: &str) -> String {
    let Some(start) = text.find(heading) else {
        return String::new();
    };
    let rest = &text[start + heading.len()..];
    match rest.find("\n## ") {
        Some(end) => rest[..end].to_string(),
        None => rest.to_string(),
    }
}

/// The rows of `## The lenses`: the lens's name and the file it reads.
fn lens_rows() -> Vec<(String, String)> {
    let skill = file("skills/review-lenses/SKILL.md");
    let mut rows = Vec::new();
    for line in section(&skill, "## The lenses").split('\n') {
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 3 || cells[1].is_empty() || cells[1].starts_with('-') || cells[1] == "lens"
        {
            continue;
        }
        rows.push((
            cells[1].trim_matches('`').to_string(),
            cells[2].trim_matches('`').to_string(),
        ));
    }
    rows
}

const LENS_DIRECTORY: &str = "skills/review-lenses";

#[test]
fn the_lens_table_and_the_directory_agree() {
    let rows = lens_rows();
    assert_eq!(rows.len(), 5, "the set is five lenses");
    let names: std::collections::HashSet<&str> = rows.iter().map(|row| row.0.as_str()).collect();
    assert_eq!(
        names,
        ["code", "security", "ui-design", "ux", "architecture"]
            .into_iter()
            .collect()
    );
    for (name, path) in &rows {
        assert!(
            root().join(path).exists(),
            "lens `{name}` names {path}, which does not exist"
        );
    }
    let directory = root().join(LENS_DIRECTORY);
    let files: Vec<String> = std::fs::read_dir(directory)
        .expect("the lens directory reads")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".md") && name != "SKILL.md")
        .collect();
    assert!(!files.is_empty(), "the lens directory holds no lens file");
    let row_files: std::collections::HashSet<String> = rows
        .iter()
        .map(|row| {
            Path::new(&row.1)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default()
        })
        .collect();
    for file in files {
        assert!(
            row_files.contains(&file),
            "{LENS_DIRECTORY}/{file} has no row in the lens table"
        );
    }
}

#[test]
fn every_lens_states_what_it_blocks_on() {
    let rows = lens_rows();
    let lens_files: Vec<&String> = rows
        .iter()
        .map(|row| &row.1)
        .filter(|path| path.starts_with(LENS_DIRECTORY))
        .collect();
    assert_eq!(
        lens_files.len(),
        4,
        "four lens files carry their own checklist"
    );
    for path in lens_files {
        let text = file(path);
        assert!(text.contains("## Read"), "{path} has no `## Read`");
        assert!(
            text.contains("## Blocks on"),
            "{path} has no `## Blocks on`"
        );
        assert!(
            text.contains("## Never blocks on"),
            "{path} has no `## Never blocks on`"
        );
        assert!(
            !section(&text, "## Blocks on").trim().is_empty(),
            "{path}'s `## Blocks on` is empty"
        );
    }
}

#[test]
fn the_gate_reads_the_tickets_lenses() {
    let reviewer = file("agents/reviewer.md");
    assert!(reviewer.contains("## Lenses"));
    assert!(reviewer.contains("`code` is the default"));
    assert!(reviewer.contains("never skipped silently"));
    assert!(reviewer.contains("skills/review-lenses/"));
}

/// A builder that hands off a red head spends the gate's first pass on a
/// reading that cannot merge, and the fix costs a second CI cycle. The
/// handoff waits for the checks on the head sha and names the run it read.
#[test]
fn the_builder_hands_off_only_on_green_checks() {
    let builder = file("agents/builder.md");
    assert!(
        builder.contains("gh pr checks"),
        "the handoff waits on `gh pr checks`"
    );
    assert!(
        builder.contains("headRefOid"),
        "the checks bind to the head sha, not the branch"
    );
    assert!(
        builder.contains("every check on the head is green"),
        "the handoff names its precondition: every check green"
    );
    assert!(
        builder.contains("run or the head sha"),
        "the ticket comment names the run or the head sha it read"
    );
}

/// The gate's pre-flight reads the checks before the diff, and a head it
/// cannot merge costs it one line, not a whole pass.
#[test]
fn the_gate_names_a_red_head_before_reading_the_diff() {
    let reviewer = file("agents/reviewer.md");
    assert!(
        reviewer.contains("red or pending"),
        "the pre-flight names a red or pending check"
    );
    assert!(
        reviewer.contains("in one line"),
        "the report on a head that cannot merge is one line"
    );
    let checks = reviewer
        .find("gh pr checks")
        .expect("the gate reads `gh pr checks`");
    let diff = reviewer
        .find("gh pr diff <n>")
        .expect("the gate reads the diff");
    assert!(
        checks < diff,
        "the checks are read in the pre-flight, before the diff"
    );
}

/// The gate's second entry point, before a build exists: one comment with the
/// five fixed questions, then the park that gives the human the window. The set
/// is fixed so two grills on one brief ask the same questions.
#[test]
fn the_gate_grills_a_brief_before_a_builder_spends_a_pass() {
    let reviewer = file("agents/reviewer.md");
    let grill = section(&reviewer, "## Grill");
    assert!(
        !grill.trim().is_empty(),
        "the prompt has a `## Grill` section"
    );
    for question in [
        "What the brief decides for the builder",
        "What it leaves to the builder's judgement",
        "Which files or surfaces it must touch",
        "Whether it fits one agent session",
        "The door class and blast radius",
    ] {
        assert!(
            grill.contains(question),
            "the grill question set dropped: {question}"
        );
    }
    assert!(
        grill.contains("ticket comment add") && grill.contains("--author reviewer"),
        "the grill lands as one comment authored `reviewer`"
    );
    assert!(
        grill.contains("--status on-hold --assignee human"),
        "the grill parks the ticket on the human"
    );
    assert!(
        grill.contains("--status ready --assignee \"\""),
        "the human's write resumes the build"
    );
    assert!(
        grill.contains("ticket answer") && grill.contains("does not apply"),
        "the grill's window is comments, not an answer block"
    );
    assert!(
        grill.contains("no diff"),
        "the grill reads no diff, because there is none"
    );
}

/// The two entries into the grill: the build's per-ticket pass and the sprint's
/// in-run refine. A ticket built on a brief the gate never read is the hole.
#[test]
fn the_build_and_sprint_skills_carry_the_grill() {
    let build = file("skills/tandem-build/SKILL.md");
    assert!(
        build.contains("`## Grill`"),
        "the build skill checks for the grill comment"
    );
    assert!(
        build.contains("two dispatches"),
        "the grill and the build are two dispatches"
    );
    assert!(
        build.contains("Never both at once"),
        "the build skill forbids dispatching both together"
    );
    let sprint = file("skills/tandem-sprint/SKILL.md");
    assert!(
        sprint.contains("carries the grill before it is drawn"),
        "a ticket shaped in-run is grilled before it is drawn"
    );
}

#[test]
fn the_refiner_writes_the_lenses_line() {
    let refiner = file("agents/refiner.md");
    assert!(refiner.contains("## Lenses"));
    assert!(refiner.contains("skills/review-lenses/"));
}

#[test]
fn the_roster_points_at_the_lens_set() {
    let team = file("skills/tandem-team/SKILL.md");
    let row = team
        .split('\n')
        .find(|line| line.starts_with("| `reviewer`"));
    assert!(
        row.is_some_and(|row| row.contains("skills/review-lenses/")),
        "the reviewer row names the lens set it reads"
    );
}

#[test]
fn conventions_state_the_scaffold() {
    let c = file("skills/tandem-memory/CONVENTIONS.md");
    assert!(c.contains("fixed scaffold"));
    for page in [
        "**Overview**",
        "**Decisions**",
        "**Risks**",
        "**Processes**",
    ] {
        assert!(c.contains(page), "missing scaffold page {page}");
    }
    assert!(c.contains("created on demand"));
    assert!(c.contains("no per-persona folders"));
}

#[test]
fn conventions_state_the_inputs_and_the_daily_notes_read_only_rule() {
    let c = file("skills/tandem-memory/CONVENTIONS.md");
    assert!(c.contains("three inputs"));
    assert!(c.contains("the ended session records' summaries"));
    assert!(c.contains("the human's daily notes"));
    assert!(c.contains("read-only to the consolidator"));
    assert!(c.contains("never edits, moves or archives"));
    assert!(c.contains("the human keeps and edits"));
}

#[test]
fn conventions_give_the_decisions_page_its_index_duty() {
    let c = file("skills/tandem-memory/CONVENTIONS.md");
    assert!(c.contains("every ADR"));
    assert!(c.contains("stays in the repo"));
}

#[test]
fn the_capture_cadence_retires_the_floor_work_note() {
    let write = file("skills/tandem-memory/WRITE-PROTOCOL.md");
    assert!(!write.contains("Floor: one `work` note"));
    assert!(!write.contains("significant unit of work finished"));
    assert!(write.contains("session end --summary"));
    let consolidation = file("skills/tandem-memory/CONSOLIDATION.md");
    assert!(!consolidation.contains("| `work` |"));
    assert!(consolidation.contains("--status ended"));
    assert!(consolidation.contains("session show"));
    for path in [
        "AGENTS.md",
        "skills/tandem-memory/SKILL.md",
        "skills/tandem/SKILL.md",
    ] {
        let c = file(path);
        assert!(!c.contains("|work|"), "{path} still lists the `work` kind");
        assert!(
            !c.contains("one `work` note"),
            "{path} still names the floor work note"
        );
    }
}

#[test]
fn the_bikeshed_holds_three_areas_and_the_session_record_is_not_one() {
    let memory = file("skills/tandem-memory/SKILL.md");
    assert!(!memory.to_lowercase().contains("two layers"));
    for area in ["saddlebag", "daily notes", "curated"] {
        assert!(memory.contains(area), "the skill names the {area} area");
    }
    assert!(memory.contains("tandem.db"));
    let conventions = file("skills/tandem-memory/CONVENTIONS.md");
    assert!(!conventions.contains("Two layers"));
    assert!(conventions.contains("Three areas"));
}

#[test]
fn the_per_event_capture_cadence_is_retired() {
    let retired = [
        "Record captures as you work",
        "at each moment",
        "at each qualifying moment",
        "never batch to session end",
        "don't batch to session end",
        "files its captures",
        "file note captures as you work",
        "a moment that would be lost when the session ends",
    ];
    for path in [
        "AGENTS.md",
        "skills/tandem-memory/SKILL.md",
        "skills/tandem-memory/WRITE-PROTOCOL.md",
        "skills/tandem-build/SKILL.md",
        "skills/tandem/SKILL.md",
        "agents/builder.md",
    ] {
        let c = file(path);
        for phrase in retired {
            assert!(
                !c.contains(phrase),
                "{path} still carries the retired cadence: {phrase}"
            );
        }
    }
    for path in [
        "AGENTS.md",
        "skills/tandem-memory/SKILL.md",
        "skills/tandem-memory/WRITE-PROTOCOL.md",
    ] {
        let c = file(path);
        assert!(c.contains("cost of re-derivation"));
        assert!(c.contains("session record") || c.contains("session end"));
    }
    let write = file("skills/tandem-memory/WRITE-PROTOCOL.md");
    assert!(write.contains("session end --summary"));
    let build = file("skills/tandem-build/SKILL.md");
    assert!(build.contains("session record"));
    let retro = file("skills/retro/SKILL.md");
    assert!(retro.contains("session record"));
}

#[test]
fn the_scaffolds_own_files_agree() {
    let consolidation = file("skills/tandem-memory/CONSOLIDATION.md");
    assert!(consolidation.contains("| an open **risk** | the **Risks** page"));
    assert!(!consolidation.contains("open or retired **risk**"));
    let conventions = file("skills/tandem-memory/CONVENTIONS.md");
    assert!(conventions.contains("one line per open risk"));
    for path in [
        "AGENTS.md",
        "skills/tandem-memory/SKILL.md",
        "skills/tandem-sprint/SKILL.md",
        "skills/tandem-tidy/SKILL.md",
        "README.md",
    ] {
        let orient = file(path);
        assert!(
            !names_a_stale_orient_phrase(&orient),
            "{path}: the orient step names the Overview"
        );
        assert!(
            orient.contains("**Overview**"),
            "{path}: the orient step names the Overview"
        );
    }
}

#[test]
fn the_stale_orient_phrases_are_apostrophe_agnostic() {
    for apostrophe in ["'", "\u{2019}", "\u{2018}"] {
        let stale = format!("orient from the project{apostrophe}s curated note");
        assert!(names_a_stale_orient_phrase(&stale));
    }
    assert!(names_a_stale_orient_phrase("orient from the Hub"));
    assert!(!names_a_stale_orient_phrase(
        "orient from the project's **Overview**"
    ));
}

#[test]
fn consolidation_reads_the_inputs_and_leaves_the_daily_notes_untouched() {
    let c = file("skills/tandem-memory/CONSOLIDATION.md");
    assert!(c.contains("daily"));
    assert!(c.contains("read-only to the consolidator"));
    assert!(c.contains("creates all four"));
    assert!(c.contains("never edits, moves or archives"));
}

#[test]
fn the_daily_notes_contract_is_stated() {
    for path in [
        "skills/tandem-consolidate/SKILL.md",
        "skills/tandem-memory/CONSOLIDATION.md",
    ] {
        let c = file(path);
        assert!(c.contains("[\"daily\"]"));
        assert!(c.contains("human-owned"));
        assert!(c.contains("read-only"));
        assert!(c.contains("Human notes folded through"));
        assert!(c.contains("never edits, moves or archives"));
    }
    let consolidation = file("skills/tandem-memory/CONSOLIDATION.md");
    assert!(consolidation.contains("not mtime"));
    let consolidate = file("skills/tandem-consolidate/SKILL.md");
    assert!(consolidate.contains("tandem-cli note capture"));
}

#[test]
fn the_wiki_is_documentation() {
    let adr = file("docs/adr/0033-wiki-is-documentation.md");
    assert!(adr.contains("## Decision"));
    assert!(adr.contains("journal"));
    assert!(adr.contains("human"));
    assert!(adr.contains("agent"));
    let conventions = file("skills/tandem-memory/CONVENTIONS.md");
    assert!(conventions.contains("## One wiki, two readers"));
    assert!(conventions.contains("ADR 0033"));
    assert!(conventions.contains("journal is out"));
    assert!(
        conventions.contains("what this is, how to run it, where things live, and what is open")
    );
}

#[test]
fn the_freshness_rule() {
    let conventions = file("skills/tandem-memory/CONVENTIONS.md");
    assert!(conventions.contains("Reviewed <date>"));
    assert!(conventions.contains("mtime"));
    assert!(conventions.contains("amends the page in the same write"));
    let agents = file("AGENTS.md");
    assert!(agents.contains("CONVENTIONS.md"));
    assert!(!agents.contains("Reviewed <date>"));
}

#[test]
fn the_readme_leads_a_human_to_a_projects_overview() {
    let readme = file("README.md");
    assert!(readme.contains("~/.tandem/bikeshed/projects/<project>/<project>.md"));
    assert!(readme.contains("`## Threads`"));
    assert!(readme.contains("orient from the project's **Overview**"));
}

fn adr_files() -> Vec<String> {
    let directory = root().join("docs/adr");
    let mut files: Vec<String> = std::fs::read_dir(directory)
        .expect("the ADR directory reads")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".md"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|name| format!("docs/adr/{name}"))
        .collect()
}

#[test]
fn a_deployment_shape_is_not_a_product_property() {
    let readme = file("README.md");
    let folded = readme.to_lowercase();
    for phrase in ["no account, no server", "no server, no network"] {
        assert!(!folded.contains(phrase), "README.md sells `{phrase}`");
    }
    assert!(readme.contains("Everything runs on your Mac today"));
    assert!(readme.contains("| Network | none today"));
    assert!(readme.contains("Not a service yet"));
    let port = file("docs/adr/0037-one-stack-rust-core-electron-ui.md");
    assert!(port.contains("deployment shape, not a feature"));
    for path in adr_files() {
        let text = file(&path).to_lowercase();
        for phrase in ["no account, no server", "no server, no network"] {
            assert!(!text.contains(phrase), "{path} sells `{phrase}`");
        }
    }
}

#[test]
fn the_setup_surfaces_run_the_one_install_verb() {
    let command = "tandem-cli setup install";
    for path in ["README.md", "app/renderer/src/faces/SettingsFace.tsx"] {
        assert!(file(path).contains(command), "{path} names `{command}`");
    }
    assert!(file("skills/tandem-setup/SKILL.md").contains(command));
    for path in [
        "README.md",
        "app/renderer/src/faces/SettingsFace.tsx",
        "skills/tandem-setup/SKILL.md",
    ] {
        assert!(
            !file(path).contains("ln -s \"$PWD/skills\""),
            "{path} still links by hand"
        );
    }
}

#[test]
fn agents_states_the_install_route() {
    let agents = file("AGENTS.md");
    assert!(!agents.contains("linked rather than copied"));
    assert!(!agents.contains("a clone of this repo, anywhere, plus the app, is the whole setup"));
    assert!(agents.contains("Contents/Resources/agent"));
    assert!(agents.contains("<TandemHome>/agent/"));
    assert!(agents.contains("tandem-cli setup install"));
    assert!(agents.contains("--force"));
}

#[test]
fn a_review_dispatches_on_its_own_pr_not_on_the_batch() {
    let build = file("skills/tandem-build/SKILL.md");
    assert!(build.contains("A review dispatches when its own PR is ready, first-come"));
    assert!(
        !build.contains("in batch order"),
        "the gate is not the batch's to order"
    );
    assert!(build.contains("Neither reads another ticket"));
    let sprint = file("skills/tandem-sprint/SKILL.md");
    assert!(sprint.contains("a slow refine holds its own ticket only"));
}

#[test]
fn the_questions_exit_is_one_write() {
    let agents = file("AGENTS.md");
    assert!(agents.contains("`human` is an assignee only while a question is open"));
    assert!(agents.contains("`ticket answer` is the one write that records the answer"));
    let sprint = file("skills/tandem-sprint/SKILL.md");
    assert!(sprint.contains("tandem-cli ticket answer"));
    assert!(sprint.contains("that is the bookkeeping debt"));
    let interview = file("skills/tandem-interview/SKILL.md");
    assert!(interview.contains("tandem-cli ticket answer"));
}
