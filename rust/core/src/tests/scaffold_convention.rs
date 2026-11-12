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

/// The rows of `## The lenses`: the lens's name, the file it reads and its owner.
fn lens_rows() -> Vec<(String, String, String)> {
    let skill = file("skills/review-lenses/SKILL.md");
    let mut rows = Vec::new();
    for line in section(&skill, "## The lenses").split('\n') {
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 5 || cells[1].is_empty() || cells[1].starts_with('-') || cells[1] == "lens"
        {
            continue;
        }
        rows.push((
            cells[1].trim_matches('`').to_string(),
            cells[2].trim_matches('`').to_string(),
            cells[3].trim_matches('`').to_string(),
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
    for (name, path, _) in &rows {
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

/// Capacity-cancelled macOS jobs read as fail in `gh pr checks` and needed
/// hand reruns across passes (TAN-297). The handoff verifies a red check's
/// conclusion through the jobs API before reading it as a code failure, and
/// reruns cancelled or failed infra jobs, cancelling the stuck run first when
/// `rerun --failed` refuses a running run.
#[test]
fn the_builder_verifies_red_checks_through_the_jobs_api() {
    let builder = file("agents/builder.md");
    assert!(
        builder.contains("cancelled") && builder.contains("as fail"),
        "the handoff names that `gh pr checks` prints a cancelled job as fail"
    );
    assert!(
        builder.contains("jobs API") || builder.contains("jobs api"),
        "a red check's conclusion is verified through the jobs API before it reads as failure"
    );
    assert!(
        builder.contains("conclusion"),
        "the verification reads the job's conclusion, not the checks display"
    );
    assert!(
        builder.contains("gh run rerun --failed"),
        "cancelled or failed infra jobs are rerun from the pre-flight"
    );
    assert!(
        builder.contains("gh run cancel"),
        "a stuck running run is cancelled first when rerun refuses it"
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

/// The reviewer's grill is removed (SKI-3): the gate acts only with a PR.
/// The human grill in refine is the only pre-build gate, so the prompt has
/// no `## Grill` section, names no grill park, posts no grill comment, and
/// parks nothing on the human.
#[test]
fn the_gate_acts_only_with_a_pr() {
    let reviewer = file("agents/reviewer.md");
    assert!(
        !reviewer.contains("## Grill"),
        "the prompt has no `## Grill` section: the reviewer acts only with a PR"
    );
    assert!(
        !reviewer.contains("two entry points"),
        "the gate has one entry point, the PR"
    );
    assert!(
        reviewer.contains("acts only with a PR"),
        "the prompt names its single entry point"
    );
    assert!(
        reviewer.contains("agents/refiner.md"),
        "the prompt names the refine human grill as the only pre-build gate"
    );
    assert!(
        !reviewer.contains("posts the `## Grill` comment"),
        "no grill comment lands on a ticket"
    );
    assert!(
        !reviewer.contains("--status on-hold --assignee human"),
        "the gate parks nothing on the human"
    );
    assert!(
        !reviewer.contains("a grill takes step 0 and step 2"),
        "no read step is shared with a grill"
    );
}

/// The build's per-ticket grill is removed with the reviewer's (SKI-3), and
/// triage reports backlog rows as needing a standalone refine pass instead of
/// drawing them. A brief the gate never grilled is no longer the hole: the
/// human grill in refine is the gate before the build.
#[test]
fn the_build_names_no_grill_and_triage_reports_backlog() {
    let build = file("skills/tandem-build/SKILL.md");
    assert!(
        !build.contains("`## Grill`"),
        "the build skill checks for no grill comment"
    );
    assert!(
        !build.contains("two dispatches"),
        "the grill and the build are no longer two dispatches"
    );
    assert!(
        !build.contains("Never both at once"),
        "no grill parks the ticket the build would claim"
    );
    let sprint = file("skills/tandem-sprint/SKILL.md");
    assert!(
        !sprint.contains("carries the grill before it is drawn"),
        "no shaped ticket is grilled before it is drawn"
    );
    assert!(
        !sprint.contains("the grill's park"),
        "triage names no grill hold"
    );
    assert!(
        sprint.contains("needs a standalone refine pass"),
        "triage reports backlog as needing a standalone refine pass"
    );
}

/// UI-13: a renderer diff gets the dev app deployed for the human before the
/// PR opens, on a scratch copy of the fixture home so the exercise writes are
/// disposable. The deploy states itself once in `agents/builder.md`, the build
/// skill points at it, and the sprint's close reaps the surviving worktree.
#[test]
fn the_builder_deploys_a_renderer_head_to_a_scratch_home() {
    let builder = file("agents/builder.md");
    for phrase in [
        "app/renderer",
        "## Test this head",
        "scratch home",
        "TANDEM_HOME",
        "outlives this session",
        "npm run dev",
        "npm run typecheck",
        "TANDEM_SHOTS_OUT",
        "Scripts/make-fixture-home.sh",
        "fixture_db.rs",
        "agents/reviewer.md",
        "UI-16",
    ] {
        assert!(builder.contains(phrase), "the builder names `{phrase}`");
    }
    assert!(
        builder.contains("never opens the real `~/.tandem`"),
        "the deployed app never opens the real home"
    );
    assert!(
        builder.contains("sign-off"),
        "the worktree survives for the human's sign-off"
    );
    assert!(
        builder.contains("git diff --name-only origin/main")
            && !builder.contains("origin/main...HEAD"),
        "the deploy trigger reads the working tree (`git diff --name-only origin/main`), not the committed range"
    );
    assert!(
        builder.contains("git rev-parse HEAD"),
        "the `## Test this head` block names the pushed head by `git rev-parse HEAD`"
    );
    let commit = builder
        .find("**Commit and push**")
        .expect("the builder commits and pushes before deploying");
    let deploy = builder
        .find("git diff --name-only origin/main")
        .expect("the deploy trigger reads the working tree");
    let head = builder
        .find("## Test this head")
        .expect("the builder posts the head under test");
    assert!(
        commit < deploy && deploy < head,
        "the order runs commit and push, then deploy and its head block, never before the commit"
    );
    let build = file("skills/tandem-build/SKILL.md");
    assert!(
        build.contains("agents/builder.md") && build.contains("deploy contract's home"),
        "the build skill points at the deploy contract's home"
    );
    let close = section(&file("skills/tandem-sprint/SKILL.md"), "## 6. Close");
    assert!(
        close.contains("worktree") && close.contains("sign-off"),
        "the close sweeps the renderer ticket's surviving worktree"
    );
}

/// The close sweep still clears an owner only where a question is open. The
/// park it names is refine's hold now (`agents/refiner.md`), closed via a
/// comment plus `set`.
#[test]
fn the_close_sweep_names_the_refine_hold() {
    let close = section(&file("skills/tandem-sprint/SKILL.md"), "## 6. Close");
    assert!(!close.contains("grill"), "the close sweep names no grill");
    assert!(
        close.contains("refine"),
        "the close sweep names refine's hold"
    );
    assert!(
        !close.contains("`ticket answer`"),
        "refine's hold no longer closes via the retired verb"
    );
    assert!(
        close.contains("comment plus `ticket_set`"),
        "refine's hold closes via a comment plus `ticket_set`"
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

/// TAN-326: the lens table gains an owner column. Every owner is the reviewer
/// or an existing specialist prompt, and the mapping is the one the brief
/// names: `code` and `ui-design` the reviewer, `security` the security reader,
/// `architecture` the architect, `ux` the ux reader.
#[test]
fn the_owners_in_the_lens_table_exist() {
    let rows = lens_rows();
    let owners: std::collections::HashMap<&str, &str> = rows
        .iter()
        .map(|(name, _, owner)| (name.as_str(), owner.as_str()))
        .collect();
    for (lens, owner) in [
        ("code", "reviewer"),
        ("security", "security"),
        ("architecture", "architect"),
        ("ux", "ux"),
        ("ui-design", "reviewer"),
    ] {
        assert_eq!(
            owners.get(lens).copied(),
            Some(owner),
            "lens `{lens}` is owned by `{owner}`"
        );
    }
    for (lens, _, owner) in &rows {
        assert!(
            owner == "reviewer" || root().join(format!("agents/{owner}.md")).exists(),
            "lens `{lens}` names owner `{owner}`, which is neither the reviewer nor a prompt"
        );
    }
}

/// TAN-326: the floor is stated once, in the lens skill. The refiner and the
/// reviewer point at it and do not restate the three-lens list.
#[test]
fn the_required_lens_floor_is_stated_once() {
    let lenses = file("skills/review-lenses/SKILL.md");
    let floor = section(&lenses, "## The floor");
    for lens in ["`code`", "`security`", "`architecture`"] {
        assert!(floor.contains(lens), "the floor names {lens}");
    }
    for path in ["agents/refiner.md", "agents/reviewer.md"] {
        let text = file(path);
        assert!(
            text.contains("skills/review-lenses/"),
            "{path} points at the lens skill"
        );
        assert!(
            !text.contains("`code`, `security`, `architecture`"),
            "{path} restates the floor instead of pointing at its one home"
        );
    }
}

/// TAN-326: the specialist posts the constraints as a ticket comment, and only
/// the refiner and humans write brief prose.
#[test]
fn the_specialist_comments_and_the_refiner_writes_the_brief() {
    let refiner = file("agents/refiner.md");
    for phrase in [
        "specialist",
        "comment log",
        "writes no design",
        "brief prose",
    ] {
        assert!(refiner.contains(phrase), "the refiner names `{phrase}`");
    }
    for path in ["agents/architect.md", "agents/security.md", "agents/ux.md"] {
        let text = file(path);
        assert!(
            text.contains("ticket comment"),
            "{path} posts a ticket comment"
        );
        assert!(text.contains("brief prose"), "{path} writes no brief prose");
        assert!(
            text.contains("no questions") || text.contains("do not grill"),
            "{path} leaves the human round to the refiner"
        );
    }
}

/// TAN-326: the gate records one line per specialist finding, as adopted,
/// rebutted or not applicable, before the verdict.
#[test]
fn the_reviewer_records_each_specialist_finding() {
    let reviewer = file("agents/reviewer.md");
    for phrase in [
        "specialist",
        "adopted",
        "rebutted",
        "not applicable",
        "## Blocks on",
    ] {
        assert!(reviewer.contains(phrase), "the gate names `{phrase}`");
    }
}

/// TAN-326: a specialist is a reader, never a verdict. It posts a PR comment,
/// never a review, and it never merges or owns a ticket.
#[test]
fn the_specialist_is_a_reader_not_a_verdict() {
    for path in ["agents/architect.md", "agents/security.md", "agents/ux.md"] {
        let text = file(path);
        assert!(text.contains("gh pr comment"), "{path} posts a PR comment");
        assert!(!text.contains("gh pr review"), "{path} posts no review");
        assert!(!text.contains("VERDICT:"), "{path} carries no verdict");
        assert!(
            text.contains("never merges") || text.contains("never merge"),
            "{path} never merges"
        );
        assert!(
            text.contains("owns no ticket") || text.contains("own no ticket"),
            "{path} owns no ticket"
        );
    }
    assert!(
        file("agents/reviewer.md").contains("VERDICT:"),
        "the gate keeps the one verdict"
    );
}

/// TAN-326: the roster names the three specialists and the lens each owns,
/// beside the four pass owners. A specialist speaks and owns no ticket.
#[test]
fn the_roster_names_the_specialists_and_their_lenses() {
    let team = file("skills/tandem-team/SKILL.md");
    for (specialist, lens) in [
        ("architect", "architecture"),
        ("security", "security"),
        ("ux", "ux"),
    ] {
        let prefix = format!("| `{specialist}`");
        let row = team.split('\n').find(|line| line.starts_with(&prefix));
        assert!(
            row.is_some_and(|row| row.contains(&format!("`{lens}`"))),
            "the roster names `{specialist}` and its `{lens}` lens"
        );
    }
    for phrase in ["owns no ticket", "never merges", "only speaks"] {
        assert!(team.contains(phrase), "the roster names `{phrase}`");
    }
    for role in ["builder", "refiner", "reviewer", "consolidator"] {
        assert!(
            team.contains(&format!("| `{role}`")),
            "the pass table keeps `{role}` as a pass owner"
        );
    }
}

/// TAN-326: the artifact's face names every prompt, and neither README nor the
/// setup skill carries the stale "four prompts" count now that three
/// specialist readers join the four pass prompts.
#[test]
fn the_agent_face_names_every_prompt() {
    let mut prompts: Vec<String> = std::fs::read_dir(root().join("agents"))
        .expect("the agents directory reads")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .map(|path| {
            path.file_stem()
                .expect("a prompt has a stem")
                .to_string_lossy()
                .to_string()
        })
        .collect();
    prompts.sort();
    assert_eq!(
        prompts,
        [
            "architect",
            "builder",
            "consolidator",
            "refiner",
            "reviewer",
            "security",
            "ux"
        ],
        "the face is the four pass prompts and the three specialist readers"
    );
    let readme = file("README.md");
    for prompt in &prompts {
        assert!(
            readme.contains(&format!("`{prompt}`")),
            "README.md names the `{prompt}` prompt"
        );
    }
    assert!(
        !readme.contains("four prompts"),
        "README.md carries no stale four-prompt count"
    );
    let setup = file("skills/tandem-setup/SKILL.md");
    assert!(
        !setup.contains("four prompts"),
        "the setup skill carries no stale four-prompt count"
    );
    assert!(
        setup.contains("specialist"),
        "the setup skill names the specialist readers"
    );
}

/// TAN-321: the claude prompt links serve `~/.claude/agents/<name>.md`, and
/// Claude reads the subagent's name from the file's own frontmatter, so every
/// prompt in the face must name itself and describe itself.
#[test]
fn every_prompt_names_itself_for_claude() {
    let mut checked = 0;
    let directory = root().join("agents");
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(&directory)
        .expect("the agents directory reads")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();
    for path in paths {
        let stem = path
            .file_stem()
            .expect("a prompt has a stem")
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(&path)
            .expect("a prompt reads")
            .replace("\r\n", "\n");
        let frontmatter = text
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---"))
            .map(|(frontmatter, _)| frontmatter)
            .unwrap_or_else(|| panic!("agents/{stem}.md opens with a frontmatter block"));
        assert!(
            frontmatter
                .lines()
                .any(|line| line == format!("name: {stem}")),
            "agents/{stem}.md names itself `{stem}` in its frontmatter"
        );
        assert!(
            frontmatter
                .lines()
                .any(|line| line.starts_with("description: ")),
            "agents/{stem}.md carries a description"
        );
        checked += 1;
    }
    assert_eq!(checked, 7, "the face carries seven prompts");
}

/// TAN-326: a specialist reads and comments, and cannot write files, ask the
/// human, or spawn another reader. None pins a model (ADR 0026's amendment).
#[test]
fn the_specialist_cannot_edit_or_spawn() {
    let mut colors = std::collections::HashSet::new();
    for path in ["agents/architect.md", "agents/security.md", "agents/ux.md"] {
        let text = file(path);
        assert!(text.contains("mode: subagent"), "{path} is a subagent");
        for action in ["edit", "question", "subagent"] {
            assert!(
                text.contains(&format!("action: {action}")),
                "{path} denies `{action}`"
            );
        }
        assert_eq!(
            text.matches("effect: deny").count(),
            3,
            "{path} denies exactly the three actions"
        );
        assert!(!text.contains("model:"), "{path} pins no model");
        let color = text
            .split('\n')
            .find(|line| line.starts_with("color:"))
            .unwrap_or_else(|| panic!("{path} names a colour"));
        assert!(
            colors.insert(color.to_string()),
            "{path} has a distinct colour"
        );
    }
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
    assert!(write.contains("`session_end`"));
    let consolidation = file("skills/tandem-memory/CONSOLIDATION.md");
    assert!(!consolidation.contains("| `work` |"));
    assert!(consolidation.contains("`status` ended"));
    assert!(consolidation.contains("`session_show`"));
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
    assert!(write.contains("`session_end`"));
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
    assert!(consolidate.contains("`note_capture`"));
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
    assert!(
        !sprint.contains("a slow refine holds its own ticket only"),
        "no refine handoff lives in the sprint: refine is standalone"
    );
}

#[test]
fn the_questions_exit_is_a_comment_plus_a_set() {
    let agents = file("AGENTS.md");
    assert!(agents.contains("`human` is an assignee only while a question is open"));
    assert!(
        !agents.contains("`ticket answer`"),
        "AGENTS.md names no retired answer verb"
    );
    assert!(
        !agents.contains("## Answer"),
        "AGENTS.md carries no Answer-block debt rule"
    );
    assert!(
        agents.contains("`ticket_set`"),
        "the pair moves via `ticket_set`"
    );
    let sprint = file("skills/tandem-sprint/SKILL.md");
    assert!(
        !sprint.contains("tandem-cli ticket answer"),
        "the sprint names no answer verb"
    );
    assert!(
        !sprint.contains("`ticket answer`"),
        "the sprint names no retired verb"
    );
    assert!(
        sprint.contains("comment plus `ticket_set`"),
        "the sprint closes holds via comment plus `ticket_set`"
    );
    let interview = file("skills/tandem-interview/SKILL.md");
    assert!(
        !interview.contains("tandem-cli ticket answer"),
        "the interview names no answer verb"
    );
    assert!(
        interview.contains("comment"),
        "the interview closes via a comment"
    );
}

/// CONTRIBUTING.md is the contributor on-ramp: it exists at the root and names
/// every gate command, so a first-time contributor's PR does not fail CI for
/// want of local guidance on what to run.
#[test]
fn the_contributing_guide_names_every_gate() {
    let guide = file("CONTRIBUTING.md");
    for phrase in [
        "cargo test",
        "cargo fmt",
        "cargo clippy",
        "npm run typecheck",
        "npm test",
    ] {
        assert!(
            guide.contains(phrase),
            "CONTRIBUTING.md never names `{phrase}`"
        );
    }
}

/// A retro finding about a project's own files files into that project's lane:
/// `Retro` keeps only the findings that belong to no project, the worktree's
/// slug resolves to the project, and the sprint skill points at the retro's
/// one lane rule instead of restating a second one.
#[test]
fn the_retro_files_into_the_projects_lane() {
    let retro = file("skills/retro/SKILL.md");
    assert!(
        retro.contains("the project's lane"),
        "the retro derives the lane from the project"
    );
    assert!(
        retro.contains("`-wt-<slug>` suffix"),
        "the worktree slug resolves to the project"
    );
    assert!(
        retro.contains("belong to no project"),
        "`Retro` keeps the findings that belong to no project"
    );
    assert!(
        retro.contains("`origin` is `retro`"),
        "the origin survives the lane move"
    );
    for path in [
        "skills/retro/SKILL.md",
        "skills/tandem-sprint/SKILL.md",
        "skills/tandem-team/SKILL.md",
    ] {
        assert!(
            !file(path).contains("Retro lane"),
            "{path} still restates the lane the retro files into"
        );
    }
    let sprint = file("skills/tandem-sprint/SKILL.md");
    assert!(
        !sprint.contains("## 6. Retro"),
        "the sprint owns no retro section"
    );
    assert!(
        sprint.contains("standalone manual pass"),
        "the sprint names retro as a standalone manual pass"
    );
    assert!(
        sprint.contains("neither runs, dispatches, nor prompts"),
        "the sprint neither runs, dispatches, nor prompts a retro"
    );
}

/// Sprint runner part 2 retired (SHE-3, ADR 0040): the app holds no
/// pass-execution effect.
///
/// TAN-281 put the pass-execution effect in `app/src/main/passExecutor.ts`,
/// speaking HTTP to the harness loopback API with Basic auth (ADR 0038) so no
/// shell-out bypassed the pin, the harness-home isolation or the fixed result
/// shape. SHE-3 removes the Workflow face and the embedded harness from the
/// app: the executor module is retired with them, and passes run outside the
/// app through the CLI and MCP. The shell stays out of the path by absence:
/// no executor module, and no pass endpoint string in main.
#[test]
fn dispatch_builds_no_shell_string() {
    assert!(
        !root().join("app/src/main/passExecutor.ts").exists(),
        "the executor module is retired: passes run outside the app (SHE-3, ADR 0040)"
    );
    let main = file("app/src/main/index.ts");
    for marker in [
        "api/sprint/pass",
        "passExecutor",
        "sprint/pass",
        "opencode serve",
        "opencode auth",
        "--stdio",
        "--port 0",
    ] {
        assert!(
            !main.contains(marker),
            "main names `{marker}`: the app ships no agent loop (SHE-3, ADR 0040)"
        );
    }
}

/// A cross-fork PR branch has no `origin` ref (TAN-298): PR #324 lives on
/// `adrien-jamelot/tandem`, so `git fetch origin <branch>` answers
/// `could not find remote ref` and the behind-count first runs against a
/// stale ref. The build skill and the gate derive the fetch remote from the
/// PR's `headRepositoryOwner` when the branch is cross-repository, and count
/// behind against the fork ref.
#[test]
fn the_build_and_gate_fetch_the_fork_remote_first() {
    let build = file("skills/tandem-build/SKILL.md");
    assert!(
        build.contains("headRepositoryOwner"),
        "the build skill derives the fetch remote from `headRepositoryOwner`"
    );
    assert!(
        build.contains("isCrossRepository"),
        "the build skill names the cross-repository branch"
    );
    assert!(
        build.contains("fork"),
        "the build skill names the fork remote it fetches first"
    );
    let reviewer = file("agents/reviewer.md");
    assert!(
        reviewer.contains("headRepositoryOwner"),
        "the gate derives the fetch remote from `headRepositoryOwner`"
    );
    assert!(
        reviewer.contains("isCrossRepository"),
        "the gate names the cross-repository branch"
    );
    assert!(
        reviewer.contains("fork"),
        "the gate names the fork remote it fetches first"
    );
}

/// SKI-3: there is no grill to skip. No finished brief skips the human round:
/// a human-origin brief goes through refine like any other ticket, with at
/// least a short confirmation round, and only refine moves a ticket to ready.
#[test]
fn a_finished_brief_gets_a_short_human_round_in_refine() {
    let build = file("skills/tandem-build/SKILL.md");
    assert!(
        !build.contains("skips the grill"),
        "the build skill names no finished-brief skip"
    );
    assert!(
        !build.contains("Sprints 1 and 2"),
        "the build skill names no Sprint 1 and 2 precedent"
    );
    let sprint = file("skills/tandem-sprint/SKILL.md");
    assert!(
        !sprint.contains("skips the grill"),
        "the sprint names no finished-brief skip"
    );
    assert!(
        !sprint.contains("Sprints 1 and 2"),
        "the sprint names no Sprint 1 and 2 precedent"
    );
    let refiner = file("agents/refiner.md");
    assert!(
        refiner.contains("short confirmation round"),
        "even a trivial or fully specified ticket gets at least a short round"
    );
    let refine = file("skills/tandem-refine/SKILL.md");
    assert!(
        refine.contains("Only refine moves a ticket to `ready`"),
        "only refine moves a ticket to ready"
    );
}

/// SKI-3: the sprint draws ready only, in stored order, capped at five.
/// Backlog is never drawn, shaped, promoted, or dispatched here: shaping is
/// the standalone refine pass's work, and triage reports each backlog row as
/// needing one.
#[test]
fn the_sprint_draws_ready_only() {
    let sprint = file("skills/tandem-sprint/SKILL.md");
    assert!(
        sprint.contains("draws `ready` only"),
        "triage draws ready only"
    );
    assert!(
        !sprint.contains("ready,backlog"),
        "backlog never fills the remaining slots"
    );
    assert!(
        sprint.contains("shapes, promotes, and dispatches nothing"),
        "the sprint shapes, promotes, and dispatches nothing"
    );
    assert!(
        !sprint.contains("The refine rule"),
        "no refine rule shapes picked backlog in-run"
    );
    assert!(
        !sprint.contains("The promotion rule"),
        "no promotion rule feeds ready from the lane"
    );
}

/// SKI-3: every refiner pass runs at least one interactive human grilling
/// round per ticket before any brief is written. A dispatched refiner relays
/// its frontier through the dispatching session and waits; the only opt-out
/// is the human's live per-case delegation, quoted in the brief; with no
/// human reachable the refiner writes no brief and parks the ticket.
#[test]
fn the_refiner_grills_the_human_per_ticket() {
    let refiner = file("agents/refiner.md");
    assert!(
        refiner.contains("before any brief is written"),
        "the human round comes before any brief"
    );
    assert!(
        refiner.contains("numbered questions"),
        "the round is the grilling skill's round format"
    );
    assert!(
        refiner.contains("relay") && refiner.contains("wait"),
        "a dispatched refiner relays its frontier and waits"
    );
    assert!(
        refiner.contains("quote"),
        "the only opt-out is quoted under Decided against"
    );
    assert!(
        refiner.contains("writes no brief"),
        "with no human reachable the refiner writes no brief"
    );
    assert!(
        refiner.contains("`assignee` human"),
        "with no human reachable the ticket parks on the human"
    );
    let refine = file("skills/tandem-refine/SKILL.md");
    assert!(
        refine.contains("relay") && refine.contains("wait"),
        "the refine skill names the relay-and-wait rule"
    );
}

/// SKI-3: retro runs only on manual invocation against a named session
/// transcript. Nothing auto-dispatches or prompts it: the sprint owns no
/// retro section and neither runs, dispatches, nor prompts one.
#[test]
fn retro_is_manual_only() {
    let retro = file("skills/retro/SKILL.md");
    assert!(
        !retro.contains("Runs at every sprint close"),
        "no run happens at sprint close"
    );
    assert!(
        !retro.contains("the sprint dispatches it"),
        "the sprint dispatches no retro"
    );
    assert!(retro.contains("manual"), "retro is a manual pass");
    assert!(
        retro.contains("named session"),
        "retro runs on a named session transcript"
    );
}

/// TAN-351: the gate reads the suite from CI on the head sha instead of
/// re-running it in a worktree. The evidence is the checks the head carries,
/// the run is resolved the way Test prep resolves it, the conditional gates
/// are named as conditional, and the old worktree re-run is gone.
#[test]
fn the_gate_reads_the_suite_from_ci_on_the_head_sha() {
    let reviewer = file("agents/reviewer.md");
    assert!(
        reviewer.contains("gh run list"),
        "the gate resolves the run the way Test prep resolves it"
    );
    assert!(
        reviewer.contains("headRefOid"),
        "the evidence binds to the head sha, not the branch"
    );
    assert!(
        reviewer.contains("the checks the head"),
        "the section names its evidence: the checks the head carries"
    );
    assert!(
        reviewer.contains("conditional"),
        "the conditional gates are named as conditional"
    );
    assert!(
        reviewer.contains("Windows") && reviewer.contains("shots (ubuntu-latest)"),
        "the section names the platform gates it must not read as universal"
    );
    assert!(
        !reviewer.contains("Run them in a throwaway worktree"),
        "the old rule is gone: no local suite re-run on a head CI has reported"
    );
}

/// TAN-351: the verdict line carries the run id and the head sha the gate
/// read, so the human's run and the gate's evidence are visibly one run. The
/// pin also holds the bar: a later edit that trades the gate's care for the
/// speed fails here.
#[test]
fn the_gate_names_the_run_and_sha_in_its_verdict() {
    let reviewer = file("agents/reviewer.md");
    let verdict = section(&reviewer, "## Verdict");
    assert!(
        verdict.contains("CI run"),
        "the verdict line names the CI run"
    );
    assert!(
        verdict.contains("<sha7>"),
        "the verdict line names seven characters of the head sha"
    );
    assert!(
        reviewer.contains("A wrong merge is the expensive error"),
        "the gate's bar survives the speed: a wrong merge is still the expensive error"
    );
}

/// TAN-351: the build skill points at the gate's evidence rule in one clause.
/// The rule's home is `agents/reviewer.md`, and a dispatch brief names no
/// re-run: one truth with a pointer, never a copy.
#[test]
fn the_build_points_at_the_gates_evidence_rule() {
    let build = file("skills/tandem-build/SKILL.md");
    let start = build
        .find("3. **Review**")
        .expect("the build skill has its review step");
    let end = build
        .find("4. **Record**")
        .expect("the build skill has its record step");
    let step = &build[start..end];
    assert!(
        step.contains("agents/reviewer.md"),
        "the review step points at the rule's home"
    );
    assert!(
        step.contains("evidence"),
        "the review step names the gate's evidence rule"
    );
    assert!(
        step.contains("head sha"),
        "the review step names the head sha the evidence binds to"
    );
}

/// SKI-3: the close cuts no release by default. A release cuts only on the
/// human's explicit per-sprint words naming the version, and the sprint never
/// prompts for one; when asked, the manual steps are today's.
/// The text between `from` and the next `to`, both named.
fn between(text: &str, from: &str, to: &str) -> String {
    let start = text
        .find(from)
        .unwrap_or_else(|| panic!("missing `{from}`"));
    let rest = &text[start + from.len()..];
    match rest.find(to) {
        Some(end) => rest[..end].to_string(),
        None => panic!("missing `{to}` after `{from}`"),
    }
}

/// A gate-blocked fix is a dispatch of its own (TAN-350): a fresh subagent
/// session with no resumed `sessionID`, its own Tandem record started before
/// the first call, the claim back, the green hand back, and the PR's branch as
/// its worktree. The build's window is never carried into it, and the build
/// skill's step numbering survives because `preflight.rs` cites step 3.
#[test]
fn the_build_names_the_fix_pass_dispatch() {
    let build = file("skills/tandem-build/SKILL.md");
    let gate = between(&build, "3. **Review**", "4. **Record**");
    for phrase in [
        "fix pass",
        "fresh subagent session",
        "names no `sessionID`",
        "`session_start`",
        "`session_link`",
        "`ticket_set`",
        "`status` in-progress",
        "`status` in-review",
        "green head bound to the new sha",
        "PR's branch",
    ] {
        assert!(
            gate.contains(phrase),
            "the gate step names `{phrase}` for the fix pass"
        );
    }
    let record = between(&build, "4. **Record**", "## Budget the turns");
    assert!(
        record.contains("fix pass's record handle"),
        "the record step names the fix pass's handle beside the build's"
    );
    assert!(
        build.contains("3. **Review**") && build.contains("4. **Record**"),
        "the step numbering survives: `preflight.rs` cites step 3 as the gate"
    );
}

/// The fix brief carries the ticket, the PR number, the head sha and the
/// quoted finding, and nothing else: the build's reasoning and its window are
/// not inputs, so a fix starts at a ticket and a finding.
#[test]
fn the_fix_brief_carries_the_gate_finding_and_nothing_else() {
    let build = file("skills/tandem-build/SKILL.md");
    let gate = between(&build, "3. **Review**", "4. **Record**");
    for phrase in [
        "four things and nothing else",
        "the ticket id",
        "the PR number",
        "the head sha",
        "the gate's finding quoted",
        "`### file:line`",
        "not retold",
        "not inputs",
    ] {
        assert!(
            gate.contains(phrase),
            "the fix brief's contract names `{phrase}`"
        );
    }
}

/// The fresh-dispatch rule is the fix's, not the gate's: a re-gate may resume
/// its own reviewer session, whose context starts at one ticket and one PR and
/// which re-reads the diff on a new push. `agents/reviewer.md` keeps that rule.
#[test]
fn the_fix_rule_is_not_the_gates() {
    let build = file("skills/tandem-build/SKILL.md");
    let gate = between(&build, "3. **Review**", "4. **Record**");
    assert!(
        gate.contains("not the gate's"),
        "the gate step keeps the rule from the gate's re-gate"
    );
    assert!(
        gate.contains("re-gate may resume its own reviewer session"),
        "a re-gate may resume its own reviewer session"
    );
    assert!(
        gate.contains("agents/reviewer.md"),
        "the boundary points at the gate's own contract"
    );
    let reviewer = file("agents/reviewer.md");
    assert!(
        reviewer.contains("a new push invalidates the earlier reading"),
        "the gate keeps its re-read-on-a-new-push rule"
    );
}

/// `agents/builder.md` states the fix shape beside its build shape: the fix
/// brief's extra fields, the PR's branch as its worktree, and the build
/// sentence, one ticket per session and the green-only handoff as written.
#[test]
fn the_builder_reads_a_fix_brief() {
    let builder = file("agents/builder.md");
    for phrase in [
        "fix brief",
        "the PR number",
        "the head sha",
        "the gate's finding",
        "the PR's branch",
        "One ticket per session",
        "every check on the head is green",
    ] {
        assert!(
            builder.contains(phrase),
            "the builder prompt names `{phrase}`"
        );
    }
    assert!(
        builder.contains("gives you the ticket id and its description"),
        "the build sentence stays: a build gets the ticket id and its description"
    );
}

#[test]
fn the_close_cuts_no_release_by_default() {
    let close = section(&file("skills/tandem-sprint/SKILL.md"), "## 6. Close");
    assert!(
        !close.contains("Cut the release here"),
        "the close cuts no release on its own"
    );
    assert!(
        close.contains("cuts no release by default"),
        "the close names its default"
    );
    assert!(
        close.contains("explicit"),
        "a release needs the human's explicit words"
    );
    assert!(
        close.contains("never prompts"),
        "the sprint never prompts for a release"
    );
    assert!(
        close.contains("Scripts/release-notes.sh"),
        "the manual release still files notes the same way"
    );
}

/// TAN-348: the turn budget is the builder's to read. The number lives once,
/// in `agents/builder.md` step 4, with the batched shapes named, and step 9
/// names the below-target duty the record feeds.
#[test]
fn the_builder_contract_states_the_turn_budget() {
    let builder = file("agents/builder.md");
    assert!(
        builder.contains("three tool calls per turn or more"),
        "the builder names the target"
    );
    for shape in [
        "one command for the checks a change needs",
        "one message for the facts a round needs",
        "one call for a loop over a list",
        "no re-read of a file already read",
    ] {
        assert!(
            builder.contains(shape),
            "the builder names the batch shape `{shape}`"
        );
    }
    assert!(
        builder.contains("the ratio is below the target"),
        "step 9 names the below-target duty"
    );
    assert!(
        builder.contains("names the repeats"),
        "the below-target summary names what to cut next time"
    );
}

/// TAN-348: the build skill measures the ratio from the record, reads the
/// previous child's ratio before the next claim, and keeps the store SQL as the
/// fallback for a record with no harness read.
#[test]
fn the_build_skill_measures_the_ratio_and_briefs_below_it() {
    let budget = section(&file("skills/tandem-build/SKILL.md"), "## Budget the turns");
    assert!(
        budget.contains("agents/builder.md"),
        "the number's one home is the builder contract"
    );
    assert!(
        budget.contains("`session_show`"),
        "the record's ratio is read with `session_show`"
    );
    assert!(
        budget.contains("Below"),
        "the dispatch rule keeps its `Below` trigger"
    );
    assert!(
        budget.contains("fallback"),
        "the store SQL stays as the fallback"
    );
    assert!(
        budget.contains("batch shape"),
        "the dispatch names the shape it briefed"
    );
}

/// The one file that documents the human's and the scripts' door: its CLI
/// verb tables and examples are the reference a human reads, and the brief
/// keeps them. A pass's act is never a CLI spelling, so the scan leaves this
/// file's documentation and pins its shape instead (`every_mcp_tool_name_in_
/// prose_resolves` proves its MCP table against the registry).
const CLI_REFERENCE: &str = "skills/tandem/SKILL.md";

/// The exception set: the human's install pass, the two CLI-only note verbs
/// (`note rename`, which no tool covers, and `note prune`, which TAN-322 landed
/// under PR #413 with no tool beside it; TAN-364 files that gap), the MCP
/// server as a config fact, and `--help` as a pointer at the CLI's own
/// documentation.
const TANDEM_CLI_EXCEPTIONS: [&str; 5] = ["setup", "note rename", "note prune", "mcp", "--help"];

/// Every top-level verb the CLI's clap enum spells (`rust/cli/src/main.rs`), so
/// the word after `tandem-cli` is classified as a command form or as a noun
/// ("on PATH", a path, a version). `rpc` is a verb like the rest; there is no
/// `usage` verb.
const TANDEM_CLI_VERBS: [&str; 12] = [
    "ticket", "lane", "project", "settings", "note", "session", "sprint", "doctor", "setup", "rpc",
    "mcp", "workflow",
];

/// Every subcommand each top-level verb spells, so a bare `verb subcommand`
/// span in agent prose classifies as the CLI command form it is. A lone verb
/// word is a noun ("the ticket surface"), and a tool name (`ticket_set`) is the
/// tool's spelling, so the bare scan needs both words to read a command.
const TANDEM_CLI_SUBCOMMANDS: [(&str, &[&str]); 9] = [
    (
        "ticket",
        &[
            "list", "show", "add", "file", "set", "move", "block", "unblock", "complete",
            "archive", "comment",
        ],
    ),
    ("lane", &["list", "add", "rename", "delete", "move"]),
    ("project", &["list", "add", "rename", "delete"]),
    (
        "note",
        &[
            "rename",
            "capture",
            "stats",
            "prune",
            "links",
            "write",
            "move",
            "create",
            "create-folder",
            "daily",
        ],
    ),
    ("session", &["start", "end", "link", "list", "show"]),
    (
        "sprint",
        &["open", "ticket", "run", "close", "list", "show"],
    ),
    ("workflow", &["list", "show", "set", "use", "delete"]),
    ("setup", &["install", "uninstall", "status"]),
    ("settings", &["get", "set"]),
];

/// Every markdown file the host serves as the agent face (ADR 0034), in a
/// stable order: the prompts, `AGENTS.md`, and every skill file, nested ones
/// included.
fn agent_face_files() -> Vec<String> {
    fn collect(directory: &Path, prefix: &str, files: &mut Vec<String>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(directory)
            .unwrap_or_else(|_| panic!("missing {prefix}"))
            .flatten()
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            if path.is_dir() {
                collect(&path, &format!("{prefix}/{name}"), files);
            } else if name.ends_with(".md") {
                files.push(format!("{prefix}/{name}"));
            }
        }
    }
    let mut files = Vec::new();
    collect(&root().join("skills"), "skills", &mut files);
    collect(&root().join("agents"), "agents", &mut files);
    files.push("AGENTS.md".to_string());
    files
}

/// Whether a classified form is one the exception set admits. Prefixed forms
/// classify as the verb (`setup`), bare spans as verb and subcommand
/// (`setup install`), so the verb half is read too; `note rename` and
/// `note prune` stay the two verbs whose own name is the exception.
fn is_cli_exception(form: &str) -> bool {
    TANDEM_CLI_EXCEPTIONS.contains(&form)
        || form
            .split_whitespace()
            .next()
            .is_some_and(|verb| TANDEM_CLI_EXCEPTIONS.contains(&verb))
}
/// that token is a CLI verb. A `tandem-cli` followed by anything else is a noun
/// (the binary on PATH, a path, a version) and is no act.
fn classify_tandem_cli_rest(rest: &str) -> Option<String> {
    let rest = rest.trim_start_matches(|character: char| {
        matches!(character, '`' | '$' | '"' | ',' | ';' | ':')
    });
    let mut words = rest
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && character != '-' && character != '_'
        })
        .filter(|word| !word.is_empty());
    let first = words.next()?;
    if first == "--help" {
        return Some("--help".to_string());
    }
    if !TANDEM_CLI_VERBS.contains(&first) {
        return None;
    }
    if first == "note"
        && let Some(second) = words.next()
        && (second == "rename" || second == "prune")
    {
        return Some(format!("note {second}"));
    }
    Some(first.to_string())
}

/// Every command form a line writes, one per `tandem-cli` occurrence: the
/// token after each, classified as a command form or as a noun. A line may
/// write more than one, so all occurrences are read, never only the first.
fn tandem_cli_forms(line: &str) -> Vec<String> {
    line.match_indices("tandem-cli")
        .filter_map(|(start, _)| classify_tandem_cli_rest(&line[start + "tandem-cli".len()..]))
        .collect()
}

/// The bare command form a line writes in a code span: a backticked span whose
/// first word is a CLI verb and whose second is one of that verb's
/// subcommands. A single verb word is a noun, and a tool name (`ticket_set`) is
/// the tool's spelling, so neither reads as a CLI act.
fn bare_cli_form(line: &str) -> Option<String> {
    for span in line.split('`').skip(1).step_by(2) {
        let mut words = span.split_whitespace();
        let Some(verb) = words.next() else {
            continue;
        };
        let Some(second) = words.next() else {
            continue;
        };
        let subcommand = second.trim_end_matches(|character: char| {
            !character.is_ascii_alphanumeric() && character != '-'
        });
        let spells_it = TANDEM_CLI_SUBCOMMANDS
            .iter()
            .any(|(name, subcommands)| *name == verb && subcommands.contains(&subcommand));
        if spells_it {
            return Some(format!("{verb} {subcommand}"));
        }
    }
    None
}

/// The inline code spans on one line that are a single identifier, the shape
/// the prose writes a tool call in.
fn backticked_tokens(line: &str) -> Vec<&str> {
    line.split('`')
        .enumerate()
        .filter_map(|(index, part)| {
            let identifier = index % 2 == 1
                && !part.is_empty()
                && part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_');
            identifier.then_some(part)
        })
        .collect()
}

/// TAN-325: a pass's spoken command is a tool call. The face carried the CLI's
/// shell spellings beside every verb, and a pass that followed them paid a
/// shell round trip the tools exist to avoid (and the CLI's stdin and flag
/// shapes have no meaning over a tool call). The scan reads both spellings the
/// face can carry: `tandem-cli`-prefixed forms, every occurrence on a line, and
/// bare `verb subcommand` forms in a code span. The exception set is the
/// human's install pass, the two CLI-only note verbs (`note rename`, and
/// `note prune` from TAN-322's PR #413 with its tool gap filed as TAN-364),
/// the MCP server as a config fact and `--help` as a documentation pointer; the
/// allowlist names it exactly, and the classifier assertions below prove which
/// forms each side admits, so the exceptions and the prose cannot drift apart
/// in either direction.
#[test]
fn no_tandem_cli_command_lines_in_agent_prose() {
    let files = agent_face_files();
    let mut offenders = Vec::new();
    for path in &files {
        if path == CLI_REFERENCE {
            continue;
        }
        for (index, line) in file(path).lines().enumerate() {
            let mut forms = tandem_cli_forms(line);
            forms.extend(bare_cli_form(line));
            for form in forms {
                if !is_cli_exception(&form) {
                    offenders.push(format!("{path}:{}: command form `{form}`", index + 1));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "agent prose names the CLI as the act; write the tool call instead:\n{}",
        offenders.join("\n")
    );

    for allowed in [
        "`tandem-cli setup install`",
        "`tandem-cli note rename --from a --to b`",
        "`tandem-cli note prune`",
        "`tandem-cli mcp`",
        "`tandem-cli --help`",
    ] {
        let form = tandem_cli_forms(allowed)
            .into_iter()
            .next()
            .expect("the exception is a command form");
        assert!(
            is_cli_exception(&form),
            "`{allowed}` classifies as `{form}`, which the allowlist must name"
        );
    }
    for flagged in [
        "`tandem-cli ticket list`",
        "`tandem-cli note capture`",
        "`tandem-cli session end`",
        "`tandem-cli doctor`",
        "`tandem-cli rpc`",
    ] {
        let form = tandem_cli_forms(flagged)
            .into_iter()
            .next()
            .expect("the form is a command");
        assert!(
            !is_cli_exception(&form),
            "`{flagged}` classifies as `{form}`, which the allowlist must not admit"
        );
    }
    assert_eq!(
        tandem_cli_forms("`tandem-cli ticket list` then `tandem-cli session end`"),
        vec!["ticket".to_string(), "session".to_string()],
        "a line's every `tandem-cli` occurrence is read, not only the first"
    );
    assert!(
        tandem_cli_forms("`tandem-cli` is on PATH").is_empty(),
        "a bare `tandem-cli` is a noun, not an act"
    );
    assert!(
        tandem_cli_forms("`command -v tandem-cli`").is_empty(),
        "the binary in a lookup is a noun, not an act"
    );

    assert_eq!(
        bare_cli_form("so the pass completes it: `ticket complete --id <id>`"),
        Some("ticket complete".to_string()),
        "a bare `verb subcommand` span is a command form"
    );
    assert!(
        !is_cli_exception(&bare_cli_form("`ticket complete`").unwrap()),
        "a bare build or review command is not an exception"
    );
    assert_eq!(
        bare_cli_form("`note prune` is the one CLI step"),
        Some("note prune".to_string())
    );
    assert!(
        bare_cli_form("the `session record` is a row").is_none(),
        "a verb plus a plain noun is prose, not a command"
    );
    assert!(
        bare_cli_form("`ticket_set` with `status` in-review").is_none(),
        "a tool name is the tool's spelling, not the CLI's"
    );

    let reference = file(CLI_REFERENCE);
    assert!(
        reference.contains("the CLI is the human's and the scripts' door"),
        "the one file left with CLI forms states whose door it documents"
    );
    assert!(
        reference.contains("tandem-cli --help"),
        "the reference keeps `--help` as the pointer at the CLI's own documentation"
    );
}

/// TAN-325: prose and the 42-tool surface are one truth. Every tool name the
/// prose writes as a tool call resolves against `tandem_core::mcp::tools()`,
/// and the CLI reference's MCP table lists the same 42 names, so a renamed or
/// retired tool fails here the way it fails the registry.
#[test]
fn every_mcp_tool_name_in_prose_resolves() {
    let tools = crate::mcp::tools();
    let names: std::collections::HashSet<&str> = tools.iter().map(|tool| tool.name).collect();
    assert_eq!(names.len(), 42, "the registry is the 42-tool surface");

    let reference = file(CLI_REFERENCE);
    let table = section(&reference, "## MCP tools");
    let rows: Vec<String> = table
        .lines()
        .filter(|line| line.starts_with('|'))
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let cell = *cells.get(1)?;
            if cell.starts_with('`') && cell.ends_with('`') && cell.len() > 2 {
                Some(cell.trim_matches('`').to_string())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(rows.len(), 42, "the MCP table lists one row per tool");
    for name in &names {
        assert!(
            rows.iter().any(|row| row == name),
            "the MCP table omits `{name}`"
        );
    }
    for row in &rows {
        assert!(
            names.contains(row.as_str()),
            "the MCP table names `{row}`, which the registry does not serve"
        );
    }

    let prefixes: std::collections::HashSet<&str> = names
        .iter()
        .filter_map(|name| name.split('_').next())
        .collect();
    let mut offenders = Vec::new();
    for path in agent_face_files() {
        if path == CLI_REFERENCE {
            continue;
        }
        for (index, line) in file(&path).lines().enumerate() {
            for token in backticked_tokens(line) {
                let Some(prefix) = token.split('_').next() else {
                    continue;
                };
                if !token.contains('_') || !prefixes.contains(prefix) {
                    continue;
                }
                if !names.contains(token) {
                    offenders.push(format!("{path}:{}: `{token}` is not a tool", index + 1));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "prose writes tool names the registry does not serve:\n{}",
        offenders.join("\n")
    );
}
