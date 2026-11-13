//! `make-fixture-home`: writes `Fixtures/port/Home/tandem.db` from the checked-in
//! seed (`Fixtures/port/tandem.seed.json`).
//!
//! **No clock, no random id.** Every ticket id and timestamp is in the seed, so
//! the same seed writes the same bytes on every machine. The one
//! value the store writes from the clock is `schema_migrations.applied_at`; it
//! is restated from the seed after the write, and the file is vacuumed and left
//! in rollback-journal mode so a read of the committed home needs no `-wal` or
//! `-shm` sidecar beside it.
//!
//! `Scripts/make-fixture-home.sh` is the wrapper a human runs; the suite
//! (`core/tests/fixture_db.rs`) runs this binary into a temp path and fails when
//! the committed file differs from what it writes.
//!
//! Usage: `make-fixture-home <seed.json> <tandem.db>`

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use rusqlite::Connection;
use serde::Deserialize;
use tandem_core::store::{Actor, Home, NewDependency, NewLane, NewProject, NewTicket, Store};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Seed {
    schema: Schema,
    lanes: Vec<Lane>,
    tickets: Vec<Ticket>,
    /// The dependency edges, as `ticket block --id <ticket> --on <on>` writes
    /// them. Absent on a seed that carries no edges.
    #[serde(default)]
    edges: Vec<Edge>,
}

/// One dependency edge: the waiting ticket and the ticket it waits on.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Edge {
    ticket: String,
    on: String,
}

/// The one column of `schema_migrations` the store stamps from the clock.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Schema {
    applied_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Lane {
    name: String,
    key: String,
    last_number: i64,
    position: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ticket {
    id: String,
    title: String,
    status: String,
    archived: bool,
    lane: Option<String>,
    number: Option<i64>,
    origin: String,
    assignee: Option<String>,
    link: Option<String>,
    position: i64,
    created_at: String,
    completed_at: Option<String>,
}

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let (Some(seed), Some(target)) = (arguments.next(), arguments.next()) else {
        eprintln!("usage: make-fixture-home <seed.json> <tandem.db>");
        return ExitCode::FAILURE;
    };
    match write(Path::new(&seed), Path::new(&target)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// Stage a home in a temp directory, write the seed into it, make the file
/// canonical, and copy it to `target`.
fn write(seed_path: &Path, target: &Path) -> Result<(), String> {
    let text = fs::read_to_string(seed_path)
        .map_err(|error| format!("could not read {}: {error}", seed_path.display()))?;
    let seed: Seed = serde_json::from_str(&text)
        .map_err(|error| format!("could not parse {}: {error}", seed_path.display()))?;

    // The staging home is scratch: the target's directory only ever receives
    // the finished file, never a backup or a journal.
    let staging = std::env::temp_dir().join(format!("tandem-fixture-home-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let home = Home::resolve(Some(&staging), None, Path::new("/nonexistent-user-home"));
    let store = Store::new(home);

    // One write, one transaction: the first write also applies the migrations,
    // so the file is exactly what this build writes for a fresh home.
    store
        .write(&Actor::Human, |write| {
            for lane in &seed.lanes {
                write.insert_lane(NewLane {
                    name: &lane.name,
                    key: &lane.key,
                    last_number: lane.last_number,
                    position: lane.position,
                    project: &lane.name,
                })?;
            }
            {
                use std::collections::HashSet;
                let mut seen = HashSet::new();
                let mut position: i64 = 0;
                for lane in &seed.lanes {
                    if seen.insert(lane.name.clone()) {
                        write.insert_project(NewProject {
                            name: &lane.name,
                            position,
                        })?;
                        position += 1;
                    }
                }
            }
            for ticket in &seed.tickets {
                write.insert_ticket(NewTicket {
                    id: &ticket.id,
                    title: &ticket.title,
                    description: None,
                    status: &ticket.status,
                    archived: ticket.archived,
                    lane: ticket.lane.as_deref(),
                    number: ticket.number,
                    origin: &ticket.origin,
                    assignee: ticket.assignee.as_deref(),
                    link: ticket.link.as_deref(),
                    position: ticket.position,
                    created_at: &ticket.created_at,
                    completed_at: ticket.completed_at.as_deref(),
                })?;
            }
            for edge in &seed.edges {
                write.insert_dependency(NewDependency {
                    ticket_id: &edge.ticket,
                    depends_on_id: &edge.on,
                })?;
            }
            Ok(())
        })
        .map_err(|error| format!("could not write the fixture home: {error}"))?;

    let database = store.database();
    canonical(&database, &seed.schema.applied_at)?;

    let bytes = fs::read(&database)
        .map_err(|error| format!("could not read {}: {error}", database.display()))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    fs::write(target, bytes)
        .map_err(|error| format!("could not write {}: {error}", target.display()))?;
    let _ = fs::remove_dir_all(&staging);
    Ok(())
}

/// Restate the one clock-stamped column from the seed, then make the file
/// canonical: no WAL, no journal, no free pages. A rollback-journal file opens
/// read-only without sidecars, which is what the committed home must do.
fn canonical(database: &Path, applied_at: &str) -> Result<(), String> {
    let connection = Connection::open(database)
        .map_err(|error| format!("could not open {}: {error}", database.display()))?;
    connection
        .execute("UPDATE schema_migrations SET applied_at = ?1", [applied_at])
        .map_err(|error| format!("could not stamp the schema seam: {error}"))?;
    connection
        .execute_batch("PRAGMA journal_mode=DELETE; VACUUM;")
        .map_err(|error| format!("could not make the fixture file canonical: {error}"))?;
    Ok(())
}
