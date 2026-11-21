//! The dispatch behind the MCP door, without a process: one typed method in,
//! one reply or error out.
//!
//! These are the claims the MCP suite cannot isolate:
//!
//! - every method in the table reads from its tag, so the tool registry and the
//!   dispatch speak one spelling;
//! - arguments that do not read as the method's params, and a method the build
//!   does not answer, are errors and never a panic;
//! - the revision is the store's, it moves once per write that changes a row,
//!   and a listing is a snapshot plus that number.

mod support;

use serde_json::{Value, json};
use support::Sandbox;
use tandem_core::domain::DomainError;
use tandem_core::rpc::execute;
use tandem_core::store::Actor;
use tandem_core::wire::rpc::{METHODS, Method};

/// One call: the tag and its arguments. The id is the test's own label, echoed
/// back by [`handle`] so a case can read which call an answer belongs to.
type Call = (u64, String, Value);

/// A home files its tickets into a project (TAN-339): one for the cases that
/// only need somewhere to put them.
fn seed_project(store: &tandem_core::store::Store) {
    handle(store, &frame(0, "project.add", json!({"name": "Main"})));
}

fn frame(id: u64, method: &str, params: Value) -> Call {
    (id, method.to_string(), params)
}

fn handle(store: &tandem_core::store::Store, call: &Call) -> Value {
    handle_as(store, call, &Actor::Human)
}

/// Answer a call through the typed dispatch, as `{id, result}` or
/// `{id, error: {code, message}}`.
fn handle_as(store: &tandem_core::store::Store, call: &Call, actor: &Actor) -> Value {
    let (id, tag, params) = call;
    match Method::from_call(tag, params.clone()).and_then(|method| execute(method, store, actor)) {
        Ok(reply) => json!({"id": id, "result": reply}),
        Err(error) => json!({"id": id, "error": {"code": error.code, "message": error.message}}),
    }
}

#[test]
fn every_method_round_trips_through_its_wire_tag() {
    for (tag, verb) in METHODS {
        let method = Method::from_call(tag, json!({}))
            .unwrap_or_else(|error| panic!("{tag} accepts an empty params object: {error:?}"));
        assert_eq!(method.tag(), *tag, "{tag} round-trips");
        assert_eq!(method.cli_verb(), *verb, "{tag} names its verb");
    }
}

/// The sprint record and the workflow's run history left the application
/// (ADR 0045): the wire speaks neither family, so an app or an agent can no
/// longer reach a record the store no longer keeps.
#[test]
fn the_sprint_and_workflow_run_methods_are_gone() {
    let leftovers: Vec<&str> = METHODS
        .iter()
        .map(|(tag, _)| *tag)
        .filter(|tag| tag.starts_with("sprint.") || tag.starts_with("workflow.run."))
        .collect();
    assert_eq!(
        leftovers,
        Vec::<&str>::new(),
        "sprint.* and workflow.run.* left the wire"
    );
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
        vec![
            "note.tree",
            "note.read",
            "note.backlinks",
            "note.search",
            // A write, but an agent's targeted one: its CLI door is `note write`.
            "note.edit",
        ],
        "a read with no CLI verb joins this list deliberately"
    );
}

/// The workflow's stage set, the settings face and the note-to-ticket write
/// left the application (ADR 0045): the wire speaks none of them, so an app or
/// an agent can no longer reach a surface the domain no longer keeps.
#[test]
fn the_workflow_settings_and_add_from_note_methods_are_gone() {
    let leftovers: Vec<&str> = METHODS
        .iter()
        .map(|(tag, _)| *tag)
        .filter(|tag| {
            tag.starts_with("workflow.")
                || tag.starts_with("settings.")
                || *tag == "ticket.addFromNote"
        })
        .collect();
    assert_eq!(
        leftovers,
        Vec::<&str>::new(),
        "workflow.*, settings.* and ticket.addFromNote left the wire"
    );
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

    seed_project(&store);

    let added = handle(
        &store,
        &frame(
            1,
            "ticket.add",
            json!({"project": "Main", "title": "First"}),
        ),
    );
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

    seed_project(&store);

    let added = handle_as(
        &store,
        &frame(
            1,
            "ticket.add",
            json!({"project": "Main", "title": "Mine", "assignee": "self", "origin": "self"}),
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
            json!({"project": "Main", "title": "Other", "assignee": "self"}),
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

    seed_project(&store);

    let refused = handle(
        &store,
        &frame(
            1,
            "ticket.add",
            json!({"project": "Main", "title": "Bodyless", "description": ""}),
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

    seed_project(&store);

    let added = handle(
        &store,
        &frame(
            1,
            "ticket.add",
            json!({"project": "Main", "title": "First"}),
        ),
    );
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
fn arguments_that_do_not_read_and_an_unknown_method_are_errors() {
    let sandbox = Sandbox::new("rpc-parse");
    let store = sandbox.store();

    seed_project(&store);

    let unreadable = handle(
        &store,
        &frame(1, "ticket.add", json!({"project": "Main", "title": 7})),
    );
    assert_eq!(unreadable["error"]["code"], "parse");
    assert!(
        unreadable["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("ticket.add"),
        "the error names the method whose params it could not read: {unreadable}"
    );

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

    let listed = handle(&store, &frame(3, "ticket.list", json!({})));
    assert!(listed["result"].is_object(), "the core is still answering");
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

    seed_project(&store);

    let added = handle(
        &store,
        &frame(1, "ticket.add", json!({"project": "Main", "title": "One"})),
    );
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

    let labels = handle(&store, &frame(6, "label.list", json!({})));
    assert_eq!(labels["result"]["revision"], revision, "label.list");
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

    seed_project(&store);

    handle(
        &store,
        &frame(1, "ticket.add", json!({"project": "Main", "title": "One"})),
    );
    let watch = store
        .watch()
        .expect("the store is watchable")
        .expect("a WAL store hands back one connection");
    let before = watch.revision().expect("the watch reads");

    handle(
        &store,
        &frame(2, "ticket.add", json!({"project": "Main", "title": "Two"})),
    );
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
    seed_project(&store);
    handle(
        &store,
        &frame(1, "ticket.add", json!({"project": "Main", "title": "One"})),
    );
    handle(
        &store,
        &frame(2, "ticket.add", json!({"project": "Main", "title": "Two"})),
    );

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

    seed_project(&store);

    let first = handle(
        &store,
        &frame(1, "ticket.add", json!({"project": "Main", "title": "One"})),
    );
    let once = store.revision().expect("the store reads");
    assert!(once > 0);
    assert_eq!(first["result"]["revision"], once);
    assert_eq!(
        store.last_revision(),
        once,
        "the receipt's revision was read inside the write's transaction"
    );

    handle(
        &store,
        &frame(2, "ticket.add", json!({"project": "Main", "title": "Two"})),
    );
    assert_eq!(store.revision().expect("the store reads"), once + 1);
}

#[test]
fn note_health_resolves_typed_links_against_the_boards_names() {
    let sandbox = Sandbox::new("rpc-note-health-typed");
    let store = sandbox.store();
    handle(&store, &frame(1, "project.add", json!({"name": "P"})));
    handle(
        &store,
        &frame(2, "label.add", json!({"name": "L", "project": "P"})),
    );
    handle(
        &store,
        &frame(
            3,
            "note.write",
            json!({"path": "daily.md", "text": "About [[label:l]] and [[project:p]] and [[label:nope]]."}),
        ),
    );
    let health = handle(&store, &frame(4, "note.health", json!({})));
    let report = &health["result"]["health"];
    assert_eq!(report["counts"]["links"], 3);
    assert_eq!(report["counts"]["broken"], 1, "{report}");
    assert_eq!(report["broken"][0]["link"], "label:nope");
    assert_eq!(report["orphans"], json!([]));
}

#[test]
fn the_note_methods_read_write_move_and_report_the_vault() {
    let sandbox = Sandbox::new("rpc-notes");
    let store = sandbox.store();
    let bikeshed = sandbox.root().join("home").join("bikeshed");
    std::fs::create_dir_all(bikeshed.join("projects")).expect("the folder is creatable");

    let wrote = handle(
        &store,
        &frame(
            1,
            "note.write",
            json!({"path": "alpha.md", "text": "hello [[beta]]"}),
        ),
    );
    assert_eq!(wrote["result"]["write"]["path"], "alpha.md");
    let hash = wrote["result"]["write"]["hash"]
        .as_str()
        .expect("the write answers its hash")
        .to_string();

    let read = handle(&store, &frame(2, "note.read", json!({"path": "alpha.md"})));
    assert_eq!(read["result"]["note"]["text"], "hello [[beta]]");
    assert_eq!(
        read["result"]["note"]["hash"], hash,
        "the read's hash is the write's"
    );

    handle(
        &store,
        &frame(3, "note.write", json!({"path": "beta.md", "text": "bee"})),
    );

    let tree = handle(&store, &frame(4, "note.tree", json!({})));
    let rows = tree["result"]["nodes"].to_string();
    assert!(rows.contains("alpha.md"), "the tree holds the note: {rows}");
    assert!(
        rows.contains("projects"),
        "the tree holds the folder: {rows}"
    );

    let backlinks = handle(
        &store,
        &frame(5, "note.backlinks", json!({"path": "beta.md"})),
    );
    assert_eq!(backlinks["result"]["backlinks"][0]["path"], "alpha.md");

    let health = handle(&store, &frame(6, "note.health", json!({})));
    assert_eq!(health["result"]["health"]["counts"]["notes"], 2);
    assert_eq!(health["result"]["health"]["counts"]["broken"], 0);
    assert_eq!(health["result"]["health"]["counts"]["unmatchedFolders"], 0);

    let moved = handle(
        &store,
        &frame(
            7,
            "note.move",
            json!({"path": "beta.md", "into": "projects"}),
        ),
    );
    assert_eq!(moved["result"]["moved"]["path"], "projects/beta.md");
    assert_eq!(moved["result"]["moved"]["oldPath"], "beta.md");
    let landed = handle(
        &store,
        &frame(8, "note.read", json!({"path": "projects/beta.md"})),
    );
    assert_eq!(landed["result"]["note"]["text"], "bee");
    let rewritten = handle(&store, &frame(9, "note.read", json!({"path": "alpha.md"})));
    assert_eq!(
        rewritten["result"]["note"]["text"], "hello [[beta]]",
        "the canonical target stays the bare name while the basename is unique"
    );
}

#[test]
fn the_note_tree_never_draws_a_hidden_entry() {
    let sandbox = Sandbox::new("rpc-note-hidden");
    let store = sandbox.store();
    let bikeshed = sandbox.root().join("home").join("bikeshed");
    std::fs::create_dir_all(bikeshed.join(".raw")).expect("the folder is creatable");
    std::fs::write(bikeshed.join(".raw").join("capture.md"), "hidden")
        .expect("the note is written");

    let curated = handle(&store, &frame(1, "note.tree", json!({})));
    assert!(
        !curated["result"]["nodes"]
            .to_string()
            .contains("capture.md"),
        "a dot component is not drawn: the reveal parameter left the wire (CUT-7)"
    );

    let asked = handle(&store, &frame(2, "note.tree", json!({"hidden": true})));
    assert!(
        !asked["result"]["nodes"].to_string().contains("capture.md"),
        "a hidden field is not a parameter any more: {asked}"
    );
}

#[test]
fn a_note_read_of_no_path_is_the_domains_own_refusal() {
    let sandbox = Sandbox::new("rpc-note-refusal");
    let store = sandbox.store();

    let answer = handle(
        &store,
        &frame(1, "note.read", json!({"path": "../escape.md"})),
    );
    assert_eq!(answer["error"]["code"], "domain");
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("escape.md"),
        "the refusal names the path: {answer}"
    );
}

#[test]
fn a_note_write_without_text_is_refused_rather_than_emptied() {
    let sandbox = Sandbox::new("rpc-note-no-text");
    let store = sandbox.store();

    handle(
        &store,
        &frame(
            1,
            "note.write",
            json!({"path": "note.md", "text": "keep me"}),
        ),
    );
    let refused = handle(&store, &frame(2, "note.write", json!({"path": "note.md"})));
    assert_eq!(refused["error"]["code"], "domain");
    assert_eq!(refused["error"]["message"], "the write carries no text");

    let read = handle(&store, &frame(3, "note.read", json!({"path": "note.md"})));
    assert_eq!(
        read["result"]["note"]["text"], "keep me",
        "the refused write left the note alone"
    );
}

#[test]
fn note_create_and_create_folder_write_through_the_domain() {
    let sandbox = Sandbox::new("rpc-note-create");
    let store = sandbox.store();
    let vault = sandbox.root().join("home").join("bikeshed");

    let created = handle(
        &store,
        &frame(1, "note.create", json!({"path": "notes/new.md"})),
    );
    assert_eq!(created["result"]["path"], "notes/new.md");
    assert_eq!(
        std::fs::read_to_string(vault.join("notes").join("new.md")).expect("the note is on disk"),
        "",
        "a created note starts empty"
    );

    let taken = handle(
        &store,
        &frame(2, "note.create", json!({"path": "notes/new.md"})),
    );
    assert_eq!(taken["error"]["code"], "domain");
    assert!(
        taken["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("already exists"),
        "a taken name is refused, never clobbered: {taken}"
    );

    let invalid = handle(
        &store,
        &frame(3, "note.create", json!({"path": "notes/bad"})),
    );
    assert_eq!(invalid["error"]["code"], "domain");

    let escape = handle(
        &store,
        &frame(4, "note.create", json!({"path": "../escape.md"})),
    );
    assert_eq!(escape["error"]["code"], "domain");

    let folder = handle(
        &store,
        &frame(5, "note.createFolder", json!({"path": "projects/new"})),
    );
    assert_eq!(folder["result"]["path"], "projects/new");
    assert!(
        vault.join("projects").join("new").is_dir(),
        "a created folder is on disk"
    );

    let folder_taken = handle(
        &store,
        &frame(6, "note.createFolder", json!({"path": "projects/new"})),
    );
    assert_eq!(folder_taken["error"]["code"], "domain");

    let folder_invalid = handle(
        &store,
        &frame(7, "note.createFolder", json!({"path": "../escape"})),
    );
    assert_eq!(folder_invalid["error"]["code"], "domain");
}

#[test]
fn project_messages_round_trip_like_the_label_ones() {
    let sandbox = Sandbox::new("rpc-project");
    let store = sandbox.store();

    let listed = handle(&store, &frame(1, "project.list", json!({})));
    assert_eq!(listed["id"], 1);
    assert_eq!(
        listed["result"]["projects"],
        json!([]),
        "an empty home lists no projects"
    );

    let added = handle(&store, &frame(2, "project.add", json!({"name": "Alpha"})));
    assert_eq!(
        added["result"]["projects"],
        json!([{"name": "Alpha", "key": "ALP", "labels": []}])
    );

    let renamed = handle(
        &store,
        &frame(3, "project.rename", json!({"from": "Alpha", "to": "Beta"})),
    );
    assert_eq!(
        renamed["result"]["projects"],
        json!([{"name": "Beta", "key": "ALP", "labels": []}]),
        "a rename moves its labels with it (none here), and keeps the key"
    );

    let deleted = handle(&store, &frame(4, "project.delete", json!({"name": "Beta"})));
    assert_eq!(
        deleted["result"]["projects"],
        json!([]),
        "an empty project deletes"
    );

    // A label needs a project (there is no implicit one), and a project rename
    // moves the label with it while the key stays the project's.
    let orphan = handle(&store, &frame(4, "label.add", json!({"name": "Cycle"})));
    assert_eq!(orphan["error"]["code"], "domain");
    handle(
        &store,
        &frame(5, "project.add", json!({"name": "Cycle", "abbr": "cyc"})),
    );
    handle(
        &store,
        &frame(5, "label.add", json!({"name": "Cycle", "project": "Cycle"})),
    );
    let grouped = handle(&store, &frame(6, "project.list", json!({})));
    assert_eq!(
        grouped["result"]["projects"],
        json!([{"name": "Cycle", "key": "CYC", "labels": ["Cycle"]}]),
        "a label sits in the project it names, and the key is the project's"
    );
    let moved = handle(
        &store,
        &frame(7, "project.rename", json!({"from": "Cycle", "to": "Beta"})),
    );
    assert_eq!(
        moved["result"]["projects"],
        json!([{"name": "Beta", "key": "CYC", "labels": ["Cycle"]}]),
        "the label moves with its project"
    );

    let refused = handle(&store, &frame(8, "project.delete", json!({"name": "Beta"})));
    assert_eq!(refused["error"]["code"], "domain");
    let message = refused["error"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("Beta"),
        "the refusal names the project: {refused}"
    );
    assert!(
        message.contains("Cycle"),
        "the refusal names its labels: {refused}"
    );
}

#[test]
fn label_move_reorders_the_declared_labels_over_the_wire() {
    let sandbox = Sandbox::new("rpc-label-move");
    let store = sandbox.store();
    handle(&store, &frame(0, "project.add", json!({"name": "P"})));
    for (id, name) in [(1, "A"), (2, "B"), (3, "C")] {
        handle(
            &store,
            &frame(id, "label.add", json!({"name": name, "project": "P"})),
        );
    }

    let moved = handle(
        &store,
        &frame(4, "label.move", json!({"name": "C", "index": 0})),
    );
    assert_eq!(
        moved["result"]["labels"],
        json!(["C", "A", "B"]),
        "C moves to the front; the receipt answers the list after the write"
    );

    let refused = handle(
        &store,
        &frame(5, "label.move", json!({"name": "Nope", "index": 0})),
    );
    assert_eq!(refused["error"]["code"], "domain");
    let message = refused["error"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("\"Nope\""),
        "the refusal names the label: {refused}"
    );
    assert!(
        message.contains("label add") && message.contains("label list"),
        "the refusal names the fix: {refused}"
    );
}

// MARK: - The vault follows the board (TAN-531)

fn vault_dir(sandbox: &Sandbox) -> std::path::PathBuf {
    sandbox.root().join("home").join("bikeshed")
}

fn write_note(store: &tandem_core::store::Store, path: &str, text: &str) {
    let wrote = handle(
        store,
        &frame(90, "note.write", json!({"path": path, "text": text})),
    );
    assert!(wrote.get("error").is_none(), "{wrote}");
}

fn read_note(sandbox: &Sandbox, path: &str) -> String {
    std::fs::read_to_string(vault_dir(sandbox).join(path)).expect("the note reads")
}

#[test]
fn project_add_and_label_add_create_their_folders() {
    let sandbox = Sandbox::new("rpc-folders-add");
    let store = sandbox.store();
    let added = handle(&store, &frame(1, "project.add", json!({"name": "P"})));
    assert!(added["result"].get("note").is_none(), "{added}");
    assert!(vault_dir(&sandbox).join("projects/P").is_dir());

    handle(
        &store,
        &frame(2, "label.add", json!({"name": "L", "project": "P"})),
    );
    assert!(vault_dir(&sandbox).join("projects/P/L").is_dir());

    // A label named like its project is the project folder: nothing new.
    handle(
        &store,
        &frame(3, "label.add", json!({"name": "p", "project": "P"})),
    );
    assert!(!vault_dir(&sandbox).join("projects/P/p").exists());
    let health = handle(&store, &frame(4, "note.health", json!({})));
    assert_eq!(health["result"]["health"]["counts"]["missingFolders"], 0);
    assert_eq!(health["result"]["health"]["missingFolders"], json!([]));
}

#[test]
fn an_existing_folder_that_matches_ignoring_case_is_reused() {
    let sandbox = Sandbox::new("rpc-folders-case");
    let store = sandbox.store();
    std::fs::create_dir_all(vault_dir(&sandbox).join("projects/p/l")).expect("staged");
    handle(&store, &frame(1, "project.add", json!({"name": "P"})));
    handle(
        &store,
        &frame(2, "label.add", json!({"name": "L", "project": "P"})),
    );
    let names: Vec<String> = std::fs::read_dir(vault_dir(&sandbox).join("projects"))
        .expect("projects reads")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert_eq!(names, vec!["p".to_string()], "no second spelling is made");
    let labels: Vec<String> = std::fs::read_dir(vault_dir(&sandbox).join("projects/p"))
        .expect("the project folder reads")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert_eq!(labels, vec!["l".to_string()]);
    let health = handle(&store, &frame(3, "note.health", json!({})));
    assert_eq!(health["result"]["health"]["counts"]["missingFolders"], 0);
}

#[test]
fn project_rename_moves_the_folder_and_rewrites_typed_links() {
    let sandbox = Sandbox::new("rpc-folders-project-rename");
    let store = sandbox.store();
    handle(&store, &frame(1, "project.add", json!({"name": "P"})));
    handle(
        &store,
        &frame(2, "label.add", json!({"name": "L", "project": "P"})),
    );
    write_note(&store, "projects/P/L/deep.md", "deep");
    write_note(
        &store,
        "daily.md",
        "[[project:P|x]] [[project:p#frag]] [[P]] `[[project:P]]`",
    );
    write_note(&store, "plain.md", "[[projects/P/L/deep]]");

    let renamed = handle(
        &store,
        &frame(3, "project.rename", json!({"from": "P", "to": "Q"})),
    );
    assert!(renamed["result"].get("note").is_none(), "{renamed}");
    assert!(!vault_dir(&sandbox).join("projects/P").exists());
    assert!(vault_dir(&sandbox).join("projects/Q/L/deep.md").is_file());
    assert_eq!(
        read_note(&sandbox, "daily.md"),
        "[[project:Q|x]] [[project:Q#frag]] [[P]] `[[project:P]]`"
    );
    assert_eq!(read_note(&sandbox, "plain.md"), "[[deep]]");
    let health = handle(&store, &frame(4, "note.health", json!({})));
    let report = &health["result"]["health"];
    assert_eq!(report["counts"]["missingFolders"], 0, "{report}");
    assert_eq!(report["counts"]["unmatchedFolders"], 0, "{report}");
    // The plain [[P]] link never resolved, and still does not.
    assert_eq!(report["counts"]["broken"], 1, "{report}");
}

#[test]
fn label_rename_moves_the_folder_and_rewrites_typed_links() {
    let sandbox = Sandbox::new("rpc-folders-label-rename");
    let store = sandbox.store();
    handle(&store, &frame(1, "project.add", json!({"name": "P"})));
    handle(
        &store,
        &frame(2, "label.add", json!({"name": "L", "project": "P"})),
    );
    write_note(&store, "projects/P/L/n.md", "n");
    write_note(
        &store,
        "daily.md",
        "[[label:L|x]] [[label:l#h]] [[project:P]]",
    );
    let renamed = handle(
        &store,
        &frame(3, "label.rename", json!({"from": "L", "to": "M"})),
    );
    assert!(renamed["result"].get("note").is_none(), "{renamed}");
    assert!(!vault_dir(&sandbox).join("projects/P/L").exists());
    assert!(vault_dir(&sandbox).join("projects/P/M/n.md").is_file());
    assert_eq!(
        read_note(&sandbox, "daily.md"),
        "[[label:M|x]] [[label:M#h]] [[project:P]]"
    );

    // Re-parent: the folder follows the label to the other project.
    handle(&store, &frame(4, "project.add", json!({"name": "R"})));
    let moved = handle(
        &store,
        &frame(
            5,
            "label.rename",
            json!({"from": "M", "to": "M", "project": "R"}),
        ),
    );
    assert!(moved.get("error").is_none(), "{moved}");
    assert!(vault_dir(&sandbox).join("projects/R/M/n.md").is_file());
    assert!(!vault_dir(&sandbox).join("projects/P/M").exists());
}

#[test]
fn a_rename_into_an_existing_folder_commits_the_board_and_answers_a_note() {
    let sandbox = Sandbox::new("rpc-folders-clash");
    let store = sandbox.store();
    handle(&store, &frame(1, "project.add", json!({"name": "P"})));
    write_note(&store, "projects/P/keep.md", "keep");
    write_note(&store, "projects/Q/other.md", "other");
    write_note(&store, "daily.md", "[[project:P]]");

    let renamed = handle(
        &store,
        &frame(2, "project.rename", json!({"from": "P", "to": "Q"})),
    );
    assert!(renamed.get("error").is_none(), "{renamed}");
    assert_eq!(
        renamed["result"]["projects"][0]["name"], "Q",
        "the board write stands"
    );
    let note = renamed["result"]["note"]
        .as_str()
        .expect("a note names the leftover");
    assert!(
        note.contains("projects/P") && note.contains("already exists"),
        "{note}"
    );
    assert!(vault_dir(&sandbox).join("projects/P/keep.md").is_file());
    assert_eq!(
        read_note(&sandbox, "daily.md"),
        "[[project:Q]]",
        "links still follow"
    );

    let health = handle(&store, &frame(3, "note.health", json!({})));
    let unmatched = health["result"]["health"]["unmatchedFolders"].to_string();
    assert!(unmatched.contains("projects/P"), "{unmatched}");
}

#[test]
fn label_delete_leaves_the_folder() {
    let sandbox = Sandbox::new("rpc-folders-delete");
    let store = sandbox.store();
    handle(&store, &frame(1, "project.add", json!({"name": "P"})));
    handle(
        &store,
        &frame(2, "label.add", json!({"name": "L", "project": "P"})),
    );
    write_note(&store, "projects/P/L/n.md", "n");
    handle(&store, &frame(3, "label.delete", json!({"name": "L"})));
    assert!(vault_dir(&sandbox).join("projects/P/L/n.md").is_file());
    let health = handle(&store, &frame(4, "note.health", json!({})));
    assert_eq!(health["result"]["health"]["counts"]["unmatchedFolders"], 1);
}

#[test]
fn a_name_that_is_not_one_path_component_gets_no_folder_and_is_listed() {
    let sandbox = Sandbox::new("rpc-folders-bad-name");
    let store = sandbox.store();
    handle(&store, &frame(1, "project.add", json!({"name": "P"})));
    let added = handle(
        &store,
        &frame(2, "label.add", json!({"name": "a/b", "project": "P"})),
    );
    assert!(added.get("error").is_none(), "{added}");
    let note = added["result"]["note"].as_str().expect("the note says why");
    assert!(
        note.contains("a/b") && note.contains("single path component"),
        "{note}"
    );
    assert!(!vault_dir(&sandbox).join("projects/P/a").exists());
    let health = handle(&store, &frame(3, "note.health", json!({})));
    assert_eq!(
        health["result"]["health"]["missingFolders"],
        json!([{"project": "P", "label": "a/b", "path": "projects/P/a/b"}])
    );
    assert_eq!(health["result"]["health"]["counts"]["missingFolders"], 1);
}
