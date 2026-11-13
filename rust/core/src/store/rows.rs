//! The rows a write inserts.
//!
//! These are the adapter's parameters, not the domain's types: PRT-5 and PRT-6
//! map the ticket and record aggregates onto them, and the wire never sees
//! them (`docs/port.md`: the messages are domain verbs, not rows). Each struct
//! names every column its insert writes; the author is the only column it does
//! not carry, because the write's [`Actor`](super::Actor) fills it.
//!
//! Timestamps are ISO-8601 UTC text (`super::timestamp`), whole seconds.

/// A cycle lane: its name, its key (the ref prefix), its issued-number counter,
/// its render position and its parent project.
pub struct NewLane<'a> {
    pub name: &'a str,
    pub key: &'a str,
    pub last_number: i64,
    pub position: i64,
    pub project: &'a str,
}

/// One project: its name and its render position. Add appends, there is no
/// move verb, and a rename keeps its position.
pub struct NewProject<'a> {
    pub name: &'a str,
    pub position: i64,
}

/// A ticket. The ref is not a column: it is the lane's key and `number`, read
/// together, so a lane move cannot leave a second truth behind.
pub struct NewTicket<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub status: &'a str,
    pub archived: bool,
    pub lane: Option<&'a str>,
    pub number: Option<i64>,
    pub origin: &'a str,
    pub assignee: Option<&'a str>,
    pub link: Option<&'a str>,
    pub position: i64,
    pub created_at: &'a str,
    pub completed_at: Option<&'a str>,
}

/// One dependency edge, both ends a ticket's UUID. A table, not a column of
/// refs: the far side (`blocks`) is the same rows read backwards.
pub struct NewDependency<'a> {
    pub ticket_id: &'a str,
    pub depends_on_id: &'a str,
}

/// One comment. Its author is the write's actor: the comment's author and the
/// mutation's author are the same fact, so the row carries it once.
pub struct NewComment<'a> {
    pub ticket_id: &'a str,
    pub at: &'a str,
    pub body: &'a str,
}

/// A sprint row, read back: the pair (lane, number) is the identity, `workflow`
/// the workflow the run belongs to, and the batch and run log stay the JSON text
/// the record writes.
pub struct SprintRow {
    pub lane: String,
    pub number: i64,
    pub workflow: String,
    pub session: Option<String>,
    pub mode: String,
    pub batch: Option<String>,
    pub runs: Option<String>,
    pub close: Option<String>,
    pub status: String,
    pub opened_at: String,
    pub closed_at: Option<String>,
}

/// One sprint: a lane and a number key it, `workflow` says which workflow the
/// run belongs to, and its batch and runs stay JSON text because they are
/// ordered lists rather than filtered columns.
pub struct NewSprint<'a> {
    pub lane: &'a str,
    pub number: i64,
    pub workflow: &'a str,
    pub session: Option<&'a str>,
    pub mode: &'a str,
    pub batch: Option<&'a str>,
    pub runs: Option<&'a str>,
    pub close: Option<&'a str>,
    pub status: &'a str,
    pub opened_at: &'a str,
    pub closed_at: Option<&'a str>,
}

/// One setting: a key and its value, the JSON-encoded value the Swift store
/// keeps.
pub struct NewSetting<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

/// One workflow: its UUID, its name, its scope and its stamps.
pub struct NewWorkflow<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub scope: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

/// One stage of a workflow: its position, key, label, role and optional
/// binding. A stage's key is stable; the row is replaced whole when a document
/// is written.
pub struct NewWorkflowStage<'a> {
    pub workflow_id: &'a str,
    pub position: i64,
    pub key: &'a str,
    pub label: &'a str,
    pub role: &'a str,
    pub binding_role: Option<&'a str>,
    pub binding_pass: Option<&'a str>,
}
