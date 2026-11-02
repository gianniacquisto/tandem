//! Ported from `SprintsTests`, `SprintGroupingTests`, `SprintDocumentsTests`,
//! `SprintMigrationTests` and `SprintVerbTests`: the record's rules, the stored
//! order, the name it renders, the grouping the faces read, and the record's
//! life through the store.

use crate::domain::error::DomainError;
use crate::domain::sprint::{Sprint, SprintMode, SprintStatus, Sprints};

use super::support::{Sandbox, TICKET_ONE, TICKET_THREE, TICKET_TWO, at};

// A `Sprints` borrows the store it was built over, so each suite builds the
// store first and keeps it in scope: a store is cheap to clone and every
// operation opens its own connection.

#[test]
fn open_derives_the_number_within_its_lane() {
    let sandbox = Sandbox::new("open");
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();

    let first = sprints
        .open(
            &actor,
            " Port ",
            SprintMode::Parallel,
            Some("ses_1"),
            Some(&at(0)),
        )
        .expect("a sprint opens");
    assert_eq!(first.value.lane, "Port", "the lane is trimmed");
    assert_eq!(first.value.number, 1);
    assert_eq!(first.value.mode, SprintMode::Parallel);
    assert_eq!(first.value.session.as_deref(), Some("ses_1"));
    assert_eq!(first.value.status, SprintStatus::Open);
    assert_eq!(first.note, None);
    assert_eq!(first.value.name(), "Sprint 1 — Port");

    let other_lane = sprints
        .open(&actor, "Tandem", SprintMode::Serial, None, Some(&at(10)))
        .expect("each lane numbers from one");
    assert_eq!(other_lane.value.number, 1, "numbering is per lane");

    let already = sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(20)))
        .expect("a re-run is a success, not a failure");
    assert_eq!(already.value.number, 1, "a lane keeps one open sprint");
    assert_eq!(
        already.note.as_deref(),
        Some("Sprint 1 — Port is already open")
    );
}

#[test]
fn a_stamp_the_store_cannot_read_is_refused_before_it_is_written() {
    let sandbox = Sandbox::new("sprint-stamp");
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();

    assert_eq!(
        sprints
            .open(
                &actor,
                "Port",
                SprintMode::Serial,
                None,
                Some("2026-10-30T12:00:00")
            )
            .unwrap_err(),
        DomainError::InvalidStamp("2026-10-30T12:00:00".to_string())
    );
    assert!(sprints.list(None, None).unwrap().is_empty());

    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(0)))
        .unwrap();
    assert_eq!(
        sprints
            .run(&actor, "Port", 1, "A note.", Some("soon"))
            .unwrap_err(),
        DomainError::InvalidStamp("soon".to_string())
    );
    assert!(sprints.show("Port", 1).unwrap().runs.is_empty());
}

#[test]
fn a_closed_sprint_is_never_renumbered_over() {
    let sandbox = Sandbox::new("numbering");
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();

    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(0)))
        .unwrap();
    sprints
        .close(&actor, "Port", 1, Some("Landed."), Some(&at(10)))
        .unwrap();
    let second = sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(20)))
        .unwrap();

    assert_eq!(
        second.value.number, 2,
        "the highest number the lane holds plus one"
    );
}

#[test]
fn ticket_appends_once_and_refuses_an_id_the_board_does_not_carry() {
    let sandbox = Sandbox::new("sprint-ticket");
    sandbox.ticket(TICKET_ONE);
    sandbox.ticket(TICKET_TWO);
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();
    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(0)))
        .unwrap();

    sprints.ticket(&actor, "Port", 1, TICKET_TWO).unwrap();
    sprints.ticket(&actor, "Port", 1, TICKET_ONE).unwrap();
    let repeated = sprints.ticket(&actor, "Port", 1, TICKET_TWO).unwrap();
    assert_eq!(
        repeated.value.batch,
        vec![TICKET_TWO.to_string(), TICKET_ONE.to_string()],
        "the stored order is the order the additions were made in"
    );

    assert_eq!(
        sprints.ticket(&actor, "Port", 1, TICKET_THREE).unwrap_err(),
        DomainError::UnknownTicketId(TICKET_THREE.to_string())
    );
    assert_eq!(
        sprints.ticket(&actor, "Port", 9, TICKET_ONE).unwrap_err(),
        DomainError::UnknownSprint {
            lane: "Port".to_string(),
            number: 9,
            known: vec![1],
        },
        "the handle is checked before the board is asked"
    );
}

#[test]
fn a_run_entry_is_dated_by_the_core_and_an_empty_note_is_refused() {
    let sandbox = Sandbox::new("sprint-run");
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();
    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(0)))
        .unwrap();

    let ran = sprints
        .run(&actor, "Port", 1, "  PRT-5 landed.  ", Some(&at(30)))
        .expect("a run entry appends");
    assert_eq!(
        ran.value.runs,
        vec![format!("{} — PRT-5 landed.", at(30))],
        "the entry is the core's stamp and a dash and the caller's words"
    );

    assert_eq!(
        sprints
            .run(&actor, "Port", 1, "   ", Some(&at(40)))
            .unwrap_err(),
        DomainError::EmptySprintNote
    );
    assert_eq!(
        sprints.show("Port", 1).unwrap().runs.len(),
        1,
        "the refused note appended nothing"
    );
    let second = sprints
        .run(&actor, "Port", 1, "A note.", Some(&at(40)))
        .unwrap();
    assert_eq!(second.value.runs.len(), 2, "and the run log appends");
}

#[test]
fn close_stamps_once_and_a_second_close_cannot_restate_it() {
    let sandbox = Sandbox::new("sprint-close");
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();
    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(0)))
        .unwrap();

    let closed = sprints
        .close(
            &actor,
            "Port",
            1,
            Some("  What landed, slipped and waits.  "),
            Some(&at(60)),
        )
        .expect("the first close stamps");
    assert_eq!(closed.value.status, SprintStatus::Closed);
    assert_eq!(
        closed.value.close.as_deref(),
        Some("What landed, slipped and waits.")
    );
    assert_eq!(closed.value.closed_at.as_deref(), Some(at(60).as_str()));

    let again = sprints
        .close(&actor, "Port", 1, Some("Something else."), Some(&at(90)))
        .expect("a second close is a success that changed nothing");
    assert_eq!(
        again.note.as_deref(),
        Some("Sprint 1 — Port is already closed")
    );
    assert_eq!(again.value, closed.value, "the first close is the fact");

    assert_eq!(
        sprints
            .close(&actor, "Port", 1, Some("  "), None)
            .unwrap_err(),
        DomainError::EmptySprintNote,
        "a close that says nothing is no text at all, not blank text"
    );
}

#[test]
fn a_sprint_closes_with_no_text() {
    let sandbox = Sandbox::new("sprint-close-note");
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();
    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(0)))
        .unwrap();

    let closed = sprints
        .close(&actor, "Port", 1, None, Some(&at(60)))
        .unwrap();
    assert_eq!(closed.value.close, None, "the stamp alone");
    assert_eq!(closed.value.status, SprintStatus::Closed);
}

#[test]
fn list_is_newest_first_with_the_filters_and_show_names_the_numbers_held() {
    let sandbox = Sandbox::new("sprint-list");
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();
    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(0)))
        .unwrap();
    sprints
        .close(&actor, "Port", 1, None, Some(&at(10)))
        .unwrap();
    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(20)))
        .unwrap();
    sprints
        .open(&actor, "Tandem", SprintMode::Parallel, None, Some(&at(30)))
        .unwrap();

    let names: Vec<String> = sprints
        .list(None, None)
        .unwrap()
        .into_iter()
        .map(|sprint| sprint.name())
        .collect();
    assert_eq!(
        names,
        vec!["Sprint 1 — Tandem", "Sprint 2 — Port", "Sprint 1 — Port"],
        "newest opened first"
    );

    let port_open: Vec<String> = sprints
        .list(Some("Port"), Some(SprintStatus::Open))
        .unwrap()
        .into_iter()
        .map(|sprint| sprint.name())
        .collect();
    assert_eq!(port_open, vec!["Sprint 2 — Port"]);

    assert_eq!(
        sprints.show("Port", 9).unwrap_err(),
        DomainError::UnknownSprint {
            lane: "Port".to_string(),
            number: 9,
            known: vec![1, 2],
        }
    );
    assert_eq!(
        sprints.show("Fleet", 1).unwrap_err().message(),
        "Unknown sprint \"Sprint 1 — Fleet\". No sprint is recorded in lane \"Fleet\" yet."
    );
    assert_eq!(
        sprints.open_in("Fleet").unwrap_err(),
        DomainError::NoOpenSprint("Fleet".to_string())
    );
    assert_eq!(sprints.open_in("Port").unwrap().number, 2);
}

#[test]
fn the_lane_only_handles_answer_the_open_record_and_the_close_target() {
    let sandbox = Sandbox::new("sprint-handles");
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();

    assert_eq!(
        sprints.open_in("Port").unwrap_err(),
        DomainError::NoOpenSprint("Port".to_string()),
        "a lane with no record has no open sprint"
    );
    assert_eq!(
        sprints.close_in("Port").unwrap_err(),
        DomainError::NoOpenSprint("Port".to_string()),
        "a lane with no record has nothing to close"
    );

    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(0)))
        .unwrap();
    sprints
        .close(&actor, "Port", 1, None, Some(&at(10)))
        .unwrap();
    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(20)))
        .unwrap();

    assert_eq!(sprints.open_in("Port").unwrap().number, 2);
    assert_eq!(
        sprints.close_in("Port").unwrap().number,
        2,
        "the open record is the close target"
    );

    sprints
        .close(&actor, "Port", 2, None, Some(&at(30)))
        .unwrap();
    assert_eq!(
        sprints.open_in("Port").unwrap_err(),
        DomainError::NoOpenSprint("Port".to_string())
    );
    assert_eq!(
        sprints.close_in("Port").unwrap().number,
        2,
        "a close re-run reaches the lane's newest record"
    );

    assert_eq!(
        sprints.open_in(" ").unwrap_err(),
        DomainError::EmptyLaneName
    );
    assert_eq!(
        sprints.close_in(" ").unwrap_err(),
        DomainError::EmptyLaneName
    );
}

#[test]
fn the_grouping_keeps_first_appearance_order_and_the_list_order_inside_a_lane() {
    let sandbox = Sandbox::new("sprint-grouping");
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();
    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(0)))
        .unwrap();
    sprints
        .open(&actor, "Tandem", SprintMode::Serial, None, Some(&at(10)))
        .unwrap();
    sprints
        .close(&actor, "Port", 1, None, Some(&at(20)))
        .unwrap();
    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(30)))
        .unwrap();

    let listed = sprints.list(None, None).unwrap();
    let grouped = Sprints::grouped_by_lane(&listed);

    let lanes: Vec<&str> = grouped.iter().map(|group| group.lane.as_str()).collect();
    assert_eq!(
        lanes,
        vec!["Port", "Tandem"],
        "the newest sprint's lane first"
    );
    let port: Vec<i64> = grouped[0]
        .sprints
        .iter()
        .map(|sprint| sprint.number)
        .collect();
    assert_eq!(port, vec![2, 1], "the list's order, kept");
    assert_eq!(grouped[1].sprints.len(), 1);
}

#[test]
fn a_sprint_survives_a_reopen_of_the_store() {
    let sandbox = Sandbox::new("sprint-reopen");
    sandbox.ticket(TICKET_ONE);
    {
        let store = sandbox.store();
        let sprints = Sprints::new(&store);
        let actor = sandbox.actor();
        sprints
            .open(
                &actor,
                "Port",
                SprintMode::Parallel,
                Some("ses_1"),
                Some(&at(0)),
            )
            .unwrap();
        sprints.ticket(&actor, "Port", 1, TICKET_ONE).unwrap();
        sprints
            .run(&actor, "Port", 1, "Landed.", Some(&at(10)))
            .unwrap();
        sprints
            .close(&actor, "Port", 1, Some("Done."), Some(&at(20)))
            .unwrap();
    }

    // A second store over the same home reads the row back, not a cache.
    let reopened = sandbox.store();
    let sprints = Sprints::new(&reopened);
    let record: Sprint = sprints.show("Port", 1).expect("the row is in the store");

    assert_eq!(record.mode, SprintMode::Parallel);
    assert_eq!(record.status, SprintStatus::Closed);
    assert_eq!(record.batch, vec![TICKET_ONE.to_string()]);
    assert_eq!(record.runs, vec![format!("{} — Landed.", at(10))]);
    assert_eq!(record.close.as_deref(), Some("Done."));
    assert_eq!(record.opened_at, at(0));
    assert_eq!(record.closed_at.as_deref(), Some(at(20).as_str()));
}

// The rpc wire's three sprint verbs (TAN-254): each case calls the method by
// its tag through one frame, the seam the app's `window.tandem.call` reads.

/// One frame through the dispatch, answered as JSON.
fn rpc(
    store: &crate::store::Store,
    actor: &crate::store::Actor,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let frame = serde_json::json!({"id": id, "method": method, "params": params}).to_string();
    let answer = crate::rpc::handle(&frame, store, actor);
    serde_json::from_str(&answer).expect("the answer is one JSON frame")
}

#[test]
fn sprint_list_answers_open_and_closed_records_per_lane() {
    let sandbox = Sandbox::new("rpc-sprint-list");
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();
    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(0)))
        .unwrap();
    sprints
        .close(&actor, "Port", 1, None, Some(&at(10)))
        .unwrap();
    sprints
        .open(&actor, "Port", SprintMode::Serial, None, Some(&at(20)))
        .unwrap();
    sprints
        .open(&actor, "Tandem", SprintMode::Parallel, None, Some(&at(30)))
        .unwrap();

    let port = rpc(
        &store,
        &actor,
        1,
        "sprint.list",
        serde_json::json!({"lane": "Port"}),
    );
    assert_eq!(port["id"], 1, "the frame echoes its id");
    let revision = port["result"]["revision"]
        .as_i64()
        .expect("the listing carries its revision");
    assert!(revision > 0, "the writes moved the revision");
    let records = port["result"]["sprints"]
        .as_array()
        .expect("the listing carries records");
    assert_eq!(records.len(), 2, "the lane's two records");
    assert_eq!(records[0]["number"], 2, "newest opened first");
    assert_eq!(records[0]["status"], "open");
    assert_eq!(records[1]["number"], 1);
    assert_eq!(records[1]["status"], "closed");

    let open = rpc(
        &store,
        &actor,
        2,
        "sprint.list",
        serde_json::json!({"lane": "Port", "status": "open"}),
    );
    assert_eq!(open["result"]["revision"], revision, "one snapshot");
    assert_eq!(open["result"]["sprints"].as_array().unwrap().len(), 1);

    let closed = rpc(
        &store,
        &actor,
        3,
        "sprint.list",
        serde_json::json!({"status": "closed"}),
    );
    let shut = closed["result"]["sprints"].as_array().unwrap();
    assert_eq!(shut.len(), 1, "the one closed record across lanes");
    assert_eq!(shut[0]["lane"], "Port");
    assert_eq!(shut[0]["number"], 1);

    let all = rpc(&store, &actor, 4, "sprint.list", serde_json::json!({}));
    assert_eq!(all["result"]["sprints"].as_array().unwrap().len(), 3);
}

#[test]
fn sprint_show_answers_the_open_record_and_its_error_when_none_is_open() {
    let sandbox = Sandbox::new("rpc-sprint-show");
    sandbox.ticket(TICKET_ONE);
    let store = sandbox.store();
    let sprints = Sprints::new(&store);
    let actor = sandbox.actor();
    sprints
        .open(
            &actor,
            "Port",
            SprintMode::Parallel,
            Some("ses_1"),
            Some(&at(0)),
        )
        .unwrap();
    sprints.ticket(&actor, "Port", 1, TICKET_ONE).unwrap();
    sprints
        .run(&actor, "Port", 1, "PRT-5 landed.", Some(&at(10)))
        .unwrap();

    let shown = rpc(
        &store,
        &actor,
        1,
        "sprint.show",
        serde_json::json!({"lane": "Port"}),
    );
    assert_eq!(shown["id"], 1);
    assert!(shown["result"]["revision"].as_i64().unwrap() > 0);
    let record = &shown["result"]["sprint"];
    assert_eq!(record["lane"], "Port");
    assert_eq!(record["number"], 1);
    assert_eq!(record["mode"], "parallel");
    assert_eq!(record["status"], "open");
    assert_eq!(record["session"], "ses_1");
    assert_eq!(record["batch"], serde_json::json!([TICKET_ONE]));
    assert_eq!(record["runs"].as_array().unwrap().len(), 1);
    assert!(
        record["runs"][0]
            .as_str()
            .unwrap()
            .contains("PRT-5 landed."),
        "the run log entry: {}",
        record["runs"][0]
    );

    sprints
        .close(&actor, "Port", 1, None, Some(&at(20)))
        .unwrap();
    let refused = rpc(
        &store,
        &actor,
        2,
        "sprint.show",
        serde_json::json!({"lane": "Port"}),
    );
    assert_eq!(refused["id"], 2);
    assert_eq!(refused["error"]["code"], "domain");
    assert_eq!(
        refused["error"]["message"],
        DomainError::NoOpenSprint("Port".to_string()).message(),
        "the CLI's own error when none is open"
    );
}

#[test]
fn sprint_open_validates_like_the_cli_and_repeats_unchanged() {
    let sandbox = Sandbox::new("rpc-sprint-open");
    let store = sandbox.store();
    let actor = sandbox.actor();

    let opened = rpc(
        &store,
        &actor,
        1,
        "sprint.open",
        serde_json::json!({"lane": "Port", "mode": "serial"}),
    );
    assert_eq!(opened["id"], 1);
    assert!(opened["result"]["revision"].as_i64().unwrap() > 0);
    assert_eq!(opened["result"]["sprint"]["number"], 1);
    assert_eq!(opened["result"]["sprint"]["mode"], "serial");
    assert_eq!(opened["result"]["sprint"]["status"], "open");
    assert!(
        opened["result"]["note"].is_null(),
        "a first open says nothing"
    );

    let again = rpc(
        &store,
        &actor,
        2,
        "sprint.open",
        serde_json::json!({"lane": "Port", "mode": "parallel"}),
    );
    assert_eq!(
        again["result"]["sprint"]["number"], 1,
        "a lane keeps one open sprint"
    );
    assert_eq!(
        again["result"]["sprint"]["mode"], "serial",
        "the stored record, not the second call's mode"
    );
    assert_eq!(
        again["result"]["note"], "Sprint 1 — Port is already open",
        "the CLI's own note on a second open"
    );

    let parallel = rpc(
        &store,
        &actor,
        3,
        "sprint.open",
        serde_json::json!({"lane": "Tandem", "mode": "parallel"}),
    );
    assert_eq!(parallel["result"]["sprint"]["number"], 1);
    assert_eq!(parallel["result"]["sprint"]["mode"], "parallel");

    let default = rpc(
        &store,
        &actor,
        4,
        "sprint.open",
        serde_json::json!({"lane": "Retro"}),
    );
    assert_eq!(
        default["result"]["sprint"]["mode"], "serial",
        "an absent mode is serial, as the CLI defaults it"
    );

    let refused = rpc(
        &store,
        &actor,
        5,
        "sprint.open",
        serde_json::json!({"lane": "Port", "mode": "sideways"}),
    );
    assert_eq!(refused["error"]["code"], "domain");
    assert_eq!(
        refused["error"]["message"],
        DomainError::InvalidSprintMode("sideways".to_string()).message()
    );

    let blank = rpc(
        &store,
        &actor,
        6,
        "sprint.open",
        serde_json::json!({"lane": "  "}),
    );
    assert_eq!(blank["error"]["code"], "domain");
    assert_eq!(
        blank["error"]["message"],
        DomainError::EmptyLaneName.message(),
        "the lane validates as the CLI's does"
    );
}
