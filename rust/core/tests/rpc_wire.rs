//! The wire's protocol, without a process: one frame in, one frame out.
//!
//! These are the claims the pipe test cannot isolate:
//!
//! - every method in the wire's table round-trips through its tag, so the
//!   TypeScript client and the Rust server speak one spelling;
//! - a frame that is not a request answers an error and never a panic, because
//!   stdin is untrusted input (the brief's `security` lens);
//! - the revision is the store's, it moves once per write that changes a row,
//!   and a listing is a snapshot plus that number.

mod support;

use serde_json::{Value, json};
use support::Sandbox;
use tandem_core::domain::DomainError;
use tandem_core::domain::session::{Ending, SessionOutcome, Sessions};
use tandem_core::store::Actor;
use tandem_core::wire::rpc::{METHODS, Request};

fn frame(id: u64, method: &str, params: Value) -> String {
    json!({"id": id, "method": method, "params": params}).to_string()
}

fn handle(store: &tandem_core::store::Store, frame: &str) -> Value {
    handle_as(store, frame, &Actor::Human)
}

fn handle_as(store: &tandem_core::store::Store, frame: &str, actor: &Actor) -> Value {
    let answer = tandem_core::rpc::handle(frame, store, actor);
    serde_json::from_str(&answer)
        .unwrap_or_else(|error| panic!("the answer is one JSON frame ({error}): {answer}"))
}

#[test]
fn every_method_round_trips_through_its_wire_tag() {
    for (tag, verb) in METHODS {
        let request: Request = serde_json::from_str(&frame(0, tag, json!({})))
            .unwrap_or_else(|error| panic!("{tag} accepts an empty params object: {error}"));
        assert_eq!(request.method.tag(), *tag, "{tag} round-trips");
        assert_eq!(request.method.cli_verb(), *verb, "{tag} names its verb");
    }
}

/// The contract is "no UI-only write": a write names a CLI verb, and a read may
/// name none. The list is spelled out so a new read is a decision a reviewer
/// reads, not a verb someone forgot.
#[test]
fn the_read_methods_without_a_cli_verb_are_named_here() {
    let reads: Vec<&str> = METHODS
        .iter()
        .filter(|(_, verb)| verb.is_empty())
        .map(|(tag, _)| *tag)
        .collect();
    assert_eq!(
        reads,
        vec!["usage.report"],
        "a read with no CLI verb joins this list deliberately"
    );
}

#[test]
fn a_usage_report_is_one_message_over_the_records() {
    let sandbox = Sandbox::new("rpc-usage");
    let store = sandbox.store();

    // An empty home answers every field a face draws, zeroed.
    let empty = handle(&store, &frame(1, "usage.report", json!({})));
    assert_eq!(empty["id"], 1);
    let report = &empty["result"]["report"];
    assert_eq!(report["records"], 0);
    assert_eq!(report["reported"], 0);
    assert_eq!(report["cost"], 0.0);
    assert_eq!(report["overlap"], 0.0);
    assert_eq!(report["day"], json!([]));
    assert_eq!(report["model"], json!([]));
    assert_eq!(report["agent"], json!([]));
    assert_eq!(report["ticket"], json!([]));

    // Two records: one stamped with usage, one that reported nothing at all.
    let sessions = Sessions::new(&store);
    let actor = Actor::Agent("builder".to_string());
    sessions
        .start(
            &actor,
            "ses_a",
            "tandem",
            Some("builder"),
            "A brief.",
            Some("2026-10-30T12:00:00Z"),
        )
        .expect("the record starts");
    sessions
        .end(
            &actor,
            "ses_a",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("What landed."),
                tokens: None,
                cost: Some(0.5),
                model: Some("deepseek-v4"),
                skills: None,
                at: Some("2026-10-30T12:05:00Z"),
            },
        )
        .expect("the record stamps");
    sessions
        .start(
            &actor,
            "ses_b",
            "tandem",
            Some("builder"),
            "A brief.",
            Some("2026-10-30T11:00:00Z"),
        )
        .expect("the record starts");
    sessions
        .end(&actor, "ses_b", Ending::new(SessionOutcome::Succeeded))
        .expect("the record stamps");

    let answer = handle(&store, &frame(2, "usage.report", json!({})));
    let report = &answer["result"]["report"];
    assert_eq!(report["records"], 2);
    assert_eq!(report["reported"], 1, "one record carries no usage");
    assert_eq!(report["cost"], 0.5);
    assert_eq!(report["agent"][0]["label"], "builder");
    assert_eq!(report["agent"][0]["barPercent"], 100.0);
    assert_eq!(report["model"][0]["label"], "deepseek-v4");
    assert_eq!(
        report["ticket"][0]["label"], "no ticket",
        "a record that carried none lands in the ticketless row"
    );
    assert_eq!(report["ticket"][0]["records"], 2);
}

#[test]
fn a_home_whose_records_predate_usage_reports_zeros_over_its_records() {
    let sandbox = Sandbox::new("rpc-usage-predates");
    let store = sandbox.store();
    let sessions = Sessions::new(&store);
    let actor = Actor::Human;
    for id in ["ses_old", "ses_older"] {
        sessions
            .start(
                &actor,
                id,
                "tandem",
                Some("builder"),
                "A brief.",
                Some("2026-10-30T12:00:00Z"),
            )
            .expect("the record starts");
        sessions
            .end(&actor, id, Ending::new(SessionOutcome::Unknown))
            .expect("the record stamps");
    }

    let answer = handle(&store, &frame(1, "usage.report", json!({})));
    let report = &answer["result"]["report"];
    assert_eq!(
        report["records"], 2,
        "the records are the coverage's denominator"
    );
    assert_eq!(report["reported"], 0, "no record reports usage");
    assert_eq!(report["cost"], 0.0, "no sum invents a cost");
    assert_eq!(report["agent"][0]["barPercent"], 0.0, "and no bar is drawn");
}

#[test]
fn a_listing_is_a_snapshot_and_a_revision() {
    let sandbox = Sandbox::new("rpc-list");
    let store = sandbox.store();

    let answer = handle(&store, &frame(1, "ticket.list", json!({})));
    assert_eq!(answer["id"], 1);
    assert_eq!(
        answer["result"]["revision"], 0,
        "no store reads as revision 0"
    );
    assert_eq!(answer["result"]["tickets"], json!([]));
}

#[test]
fn a_mutation_moves_the_revision_and_the_next_listing_carries_it() {
    let sandbox = Sandbox::new("rpc-mutate");
    let store = sandbox.store();

    let added = handle(&store, &frame(1, "ticket.add", json!({"title": "First"})));
    let written = added["result"]["revision"]
        .as_i64()
        .unwrap_or_else(|| panic!("the mutation answers a revision: {added}"));
    assert!(written > 0, "the write moved the revision");
    assert_eq!(added["result"]["ticket"]["title"], "First");
    assert_eq!(added["result"]["ticket"]["status"], "backlog");

    let listed = handle(&store, &frame(2, "ticket.list", json!({})));
    assert_eq!(listed["result"]["revision"], written);
    assert_eq!(listed["result"]["tickets"][0]["title"], "First");
    assert_eq!(
        listed["result"]["tickets"][0]["descriptionLength"], 0,
        "a list row is the summary shape"
    );
}

#[test]
fn self_is_the_agent_on_the_wire_and_the_human_is_refused() {
    let sandbox = Sandbox::new("rpc-self");
    let store = sandbox.store();
    let reviewer = Actor::Agent("reviewer".to_string());

    let added = handle_as(
        &store,
        &frame(
            1,
            "ticket.add",
            json!({"title": "Mine", "assignee": "self", "origin": "self"}),
        ),
        &reviewer,
    );
    assert_eq!(added["result"]["ticket"]["assignee"], "reviewer");
    assert_eq!(added["result"]["ticket"]["origin"], "reviewer");

    let listed = handle_as(
        &store,
        &frame(2, "ticket.list", json!({"assignee": "self"})),
        &reviewer,
    );
    assert_eq!(
        listed["result"]["tickets"][0]["title"], "Mine",
        "the filter resolves self the same way the write did"
    );

    // Without an agent there is no name `self` can mean, and the refusal is the
    // CLI's own (`AgentNameUnset`).
    let refused = handle(
        &store,
        &frame(
            3,
            "ticket.add",
            json!({"title": "Other", "assignee": "self"}),
        ),
    );
    assert_eq!(refused["error"]["code"], "domain");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("TANDEM_AGENT"),
        "the refusal names the variable: {refused}"
    );
}

#[test]
fn an_empty_description_on_add_is_the_cli_refusal() {
    let sandbox = Sandbox::new("rpc-empty-description");
    let store = sandbox.store();

    let refused = handle(
        &store,
        &frame(
            1,
            "ticket.add",
            json!({"title": "Bodyless", "description": ""}),
        ),
    );
    assert_eq!(refused["error"]["code"], "domain");
    assert_eq!(
        refused["error"]["message"],
        DomainError::EmptyDescriptionBody.message(),
        "the wire answers the CLI's own refusal"
    );

    let listed = handle(&store, &frame(2, "ticket.list", json!({})));
    assert_eq!(
        listed["result"]["tickets"],
        json!([]),
        "the refused write left the board alone"
    );
}

#[test]
fn a_no_op_write_does_not_move_the_revision() {
    let sandbox = Sandbox::new("rpc-noop");
    let store = sandbox.store();

    let added = handle(&store, &frame(1, "ticket.add", json!({"title": "First"})));
    let written = added["result"]["revision"].as_i64().expect("a revision");

    // `ticket set` with no field names a ticket and changes nothing: a success
    // that says so, and a revision that stays put because the store did.
    let unchanged = handle(
        &store,
        &frame(
            2,
            "ticket.set",
            json!({"id": added["result"]["ticket"]["id"]}),
        ),
    );
    assert_eq!(unchanged["result"]["revision"], written);
    assert_eq!(unchanged["result"]["ticket"]["title"], "First");
}

#[test]
fn a_frame_that_is_not_json_answers_a_parse_error() {
    let sandbox = Sandbox::new("rpc-parse");
    let store = sandbox.store();

    let refused = handle(&store, "{ not json");
    assert!(refused["id"].is_null());
    assert_eq!(refused["error"]["code"], "parse");
    assert!(
        !refused["error"]["message"]
            .as_str()
            .unwrap_or("")
            .is_empty()
    );

    // The frame is not a request at all: no id, and the error still names the
    // method slot so a client can tell the two parse failures apart.
    let empty = handle(&store, "{}");
    assert!(empty["id"].is_null());
    assert_eq!(empty["error"]["code"], "parse");

    let listed = handle(&store, &frame(3, "ticket.list", json!({})));
    assert!(listed["result"].is_object(), "the core is still answering");
}

#[test]
fn an_unknown_method_names_itself_and_echoes_the_id() {
    let sandbox = Sandbox::new("rpc-method");
    let store = sandbox.store();

    let refused = handle(&store, &frame(4, "ticket.bogus", json!({})));
    assert_eq!(refused["id"], 4);
    assert_eq!(refused["error"]["code"], "method");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("ticket.bogus"),
        "the error names the method it could not answer: {refused}"
    );
}

#[test]
fn a_domain_failure_answers_an_error_and_leaves_the_store_alone() {
    let sandbox = Sandbox::new("rpc-domain");
    let store = sandbox.store();

    let refused = handle(&store, &frame(1, "ticket.show", json!({"id": "nope"})));
    assert_eq!(refused["id"], 1);
    assert_eq!(refused["error"]["code"], "domain");
    assert!(
        !refused["error"]["message"]
            .as_str()
            .unwrap_or("")
            .is_empty(),
        "the domain's words reach the client"
    );
}

#[test]
fn every_read_reply_carries_the_revision_of_its_own_snapshot() {
    let sandbox = Sandbox::new("rpc-read-revisions");
    let store = sandbox.store();

    let added = handle(&store, &frame(1, "ticket.add", json!({"title": "One"})));
    let id = added["result"]["ticket"]["id"].as_str().expect("an id");
    handle(
        &store,
        &frame(2, "ticket.comment.add", json!({"id": id, "body": "A note"})),
    );

    // No write between these reads, so every reply must name the same
    // revision, and it is the one the listing is a snapshot of.
    let listed = handle(&store, &frame(3, "ticket.list", json!({})));
    let revision = listed["result"]["revision"].clone();
    assert_ne!(revision, json!(0));

    let shown = handle(&store, &frame(4, "ticket.show", json!({"id": id})));
    assert_eq!(shown["result"]["revision"], revision, "ticket.show");
    assert_eq!(shown["result"]["ticket"]["title"], "One");

    let comments = handle(&store, &frame(5, "ticket.comment.list", json!({"id": id})));
    assert_eq!(
        comments["result"]["revision"], revision,
        "ticket.comment.list"
    );
    assert_eq!(comments["result"]["comments"][0]["body"], "A note");

    let lanes = handle(&store, &frame(6, "lane.list", json!({})));
    assert_eq!(lanes["result"]["revision"], revision, "lane.list");
}

#[test]
fn a_watch_connection_sees_a_later_write_without_reopening() {
    let sandbox = Sandbox::new("rpc-watch");
    let store = sandbox.store();
    assert!(
        store
            .watch()
            .expect("a missing store is not an error")
            .is_none(),
        "there is nothing to watch before the first write"
    );

    handle(&store, &frame(1, "ticket.add", json!({"title": "One"})));
    let watch = store
        .watch()
        .expect("the store is watchable")
        .expect("a WAL store hands back one connection");
    let before = watch.revision().expect("the watch reads");

    handle(&store, &frame(2, "ticket.add", json!({"title": "Two"})));
    assert!(
        watch.revision().expect("the watch reads") > before,
        "the reused connection sees a commit made after it opened"
    );
}

#[cfg(unix)]
#[test]
fn a_watch_notices_the_store_was_replaced() {
    let sandbox = Sandbox::new("rpc-watch-restore");
    let store = sandbox.store();
    handle(&store, &frame(1, "ticket.add", json!({"title": "One"})));
    handle(&store, &frame(2, "ticket.add", json!({"title": "Two"})));

    let watch = store
        .watch()
        .expect("the store is watchable")
        .expect("a WAL store hands back one connection");
    assert!(watch.revision().expect("the watch reads") > 0);

    // ADR 0031's restore: remove the journal files and the database, then move
    // the backup over the path. The held connection must notice that the path
    // now names another file instead of reading the retired inode.
    let home = sandbox.home().directory().to_path_buf();
    let _ = std::fs::remove_file(home.join("tandem.db-wal"));
    let _ = std::fs::remove_file(home.join("tandem.db-shm"));
    std::fs::remove_file(home.join("tandem.db")).expect("the database is removable");
    std::fs::rename(home.join("tandem.db.bak"), home.join("tandem.db"))
        .expect("the backup moves over the path");

    assert!(
        watch.revision().is_err(),
        "a watch whose file was replaced answers an error and reopens"
    );
}

#[test]
fn the_revision_moves_once_per_write_that_changes_a_row() {
    let sandbox = Sandbox::new("rpc-revision");
    let store = sandbox.store();
    assert_eq!(store.revision().expect("a fresh home reads"), 0);

    let first = handle(&store, &frame(1, "ticket.add", json!({"title": "One"})));
    let once = store.revision().expect("the store reads");
    assert!(once > 0);
    assert_eq!(first["result"]["revision"], once);
    assert_eq!(
        store.last_revision(),
        once,
        "the receipt's revision was read inside the write's transaction"
    );

    handle(&store, &frame(2, "ticket.add", json!({"title": "Two"})));
    assert_eq!(store.revision().expect("the store reads"), once + 1);
}
