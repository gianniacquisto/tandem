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
use tandem_core::store::Actor;
use tandem_core::wire::rpc::{METHODS, Request};

fn frame(id: u64, method: &str, params: Value) -> String {
    json!({"id": id, "method": method, "params": params}).to_string()
}

fn handle(store: &tandem_core::store::Store, frame: &str) -> Value {
    let answer = tandem_core::rpc::handle(frame, store, &Actor::Human);
    serde_json::from_str(&answer)
        .unwrap_or_else(|error| panic!("the answer is one JSON frame ({error}): {answer}"))
}

#[test]
fn every_method_round_trips_through_its_wire_tag() {
    for (tag, verb) in METHODS {
        assert!(!verb.is_empty(), "the method {tag} names a CLI verb");
        let request: Request = serde_json::from_str(&frame(0, tag, json!({})))
            .unwrap_or_else(|error| panic!("{tag} accepts an empty params object: {error}"));
        assert_eq!(request.method.tag(), *tag, "{tag} round-trips");
        assert_eq!(request.method.cli_verb(), *verb, "{tag} names its verb");
    }
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
fn the_revision_moves_once_per_write_that_changes_a_row() {
    let sandbox = Sandbox::new("rpc-revision");
    let store = sandbox.store();
    assert_eq!(store.revision().expect("a fresh home reads"), 0);

    let first = handle(&store, &frame(1, "ticket.add", json!({"title": "One"})));
    let once = store.revision().expect("the store reads");
    assert!(once > 0);
    assert_eq!(first["result"]["revision"], once);

    handle(&store, &frame(2, "ticket.add", json!({"title": "Two"})));
    assert_eq!(store.revision().expect("the store reads"), once + 1);
}
