//! The store's schema and its migration seam.
//!
//! **Version 1 is a fresh schema, written once.** It mirrors today's v5 tables
//! as they are *used*, not as they are named (docs/port.md, Store): the ticket
//! domain keeps its lane, its per-lane number and its dependency edge; the
//! records domains keep their rows; every mutation row carries its `author`, a
//! column that cannot be retrofitted onto a store that assumed one user.
//!
//! `schema_migrations` is the seam, ported from `StoreSchema.swift`: one row
//! per applied migration, the version read **inside** the transaction that
//! records it, so two connections opening a fresh home serialise and apply each
//! migration once. A migration that fails rolls its own transaction back and
//! fails the open; nothing half-applies.
//!
//! **A v5 file is refused, never read.** The port has no dual writer and no v5
//! reader (docs/port.md, Migration): a file whose seam is newer than this build
//! answers one error pointing at the one-off import's record (PRT-18, ADR 0031,
//! the git history), and exits non-zero.
//!
//! All SQL in the port lives under this module.

use rusqlite::{Connection, params};

use super::{Actor, Result, StoreError, timestamp, translate};

/// The version this build writes and reads. Always the last version applied, so
/// the constant is the migration count.
pub const CURRENT_VERSION: i64 = 10;

/// One migration: its version, a name for the error text, and its DDL.
pub(crate) struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub apply: fn(&Connection) -> rusqlite::Result<()>,
}

/// Every migration, in order. Append only: a shipped version's DDL never
/// changes, so a later shape lands as a new migration rather than an edit.
pub(crate) const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "the store",
        apply: version_one,
    },
    Migration {
        version: 2,
        name: "the board and comment indexes",
        apply: version_two,
    },
    Migration {
        version: SKILLS_VERSION,
        name: "the session skill counts",
        apply: version_three,
    },
    Migration {
        version: REVISION_VERSION,
        name: "the store's revision",
        apply: version_four,
    },
    Migration {
        version: HARNESS_VERSION,
        name: "the session's harness pointer",
        apply: version_five,
    },
    Migration {
        version: PROJECTS_VERSION,
        name: "the projects and the lane's parent",
        apply: version_six,
    },
    Migration {
        version: WORKFLOWS_VERSION,
        name: "the workflows and their stages",
        apply: version_seven,
    },
    Migration {
        version: TURNS_VERSION,
        name: "the session's turns and calls",
        apply: version_eight,
    },
    Migration {
        version: SELECTED_VERSION,
        name: "the selected intake stage",
        apply: version_nine,
    },
    Migration {
        version: WORKFLOW_RUNS_VERSION,
        name: "the workflow runs and the orphan-open sweep",
        apply: version_ten,
    },
];

/// Version 1: the tickets, the edge, the comments, the records, the lanes and
/// the settings. `tickets.position` is the stored order (ADR 0006) and
/// `tickets.number` the per-lane number; the ref (`PRT-4`) is derived from the
/// lane's key and that number on read, never a second stored truth.
///
/// A comment is a row: its sidecar (`comments/<id>.jsonl`) has no table,
/// because the only reader of the old file is the one-off import (PRT-18) and
/// a table beside the rows would be a second truth. `comments` keeps its
/// `author` because a comment's author is the comment's, and
/// `schema_migrations` keeps the actor that moved the schema.
const VERSION_ONE: &str = "
CREATE TABLE IF NOT EXISTS tickets (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    description  TEXT,
    status       TEXT NOT NULL,
    archived     INTEGER NOT NULL,
    lane         TEXT,
    number       INTEGER,
    origin       TEXT NOT NULL,
    assignee     TEXT,
    link         TEXT,
    position     INTEGER NOT NULL,
    created_at   TEXT NOT NULL,
    completed_at TEXT,
    author       TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS ticket_dependencies (
    ticket_id     TEXT NOT NULL,
    depends_on_id TEXT NOT NULL,
    author        TEXT NOT NULL,
    PRIMARY KEY (ticket_id, depends_on_id)
);
CREATE TABLE IF NOT EXISTS comments (
    ticket_id TEXT NOT NULL,
    author    TEXT NOT NULL,
    at        TEXT NOT NULL,
    body      TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    id                 TEXT PRIMARY KEY,
    project            TEXT NOT NULL,
    agent              TEXT,
    brief              TEXT NOT NULL,
    status             TEXT NOT NULL,
    outcome            TEXT,
    summary            TEXT,
    started_at         TEXT NOT NULL,
    ended_at           TEXT,
    tokens_input       INTEGER,
    tokens_output      INTEGER,
    tokens_reasoning   INTEGER,
    tokens_cache_read  INTEGER,
    tokens_cache_write INTEGER,
    seconds            REAL,
    cost               REAL,
    model              TEXT,
    tickets            TEXT,
    author             TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sprints (
    lane      TEXT NOT NULL,
    number    INTEGER NOT NULL,
    session   TEXT,
    mode      TEXT NOT NULL,
    batch     TEXT,
    runs      TEXT,
    close     TEXT,
    status    TEXT NOT NULL,
    opened_at TEXT NOT NULL,
    closed_at TEXT,
    author    TEXT NOT NULL,
    PRIMARY KEY (lane, number)
);
CREATE TABLE IF NOT EXISTS lanes (
    name        TEXT PRIMARY KEY,
    key         TEXT NOT NULL,
    last_number INTEGER NOT NULL DEFAULT 0,
    position    INTEGER NOT NULL,
    author      TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL,
    author TEXT NOT NULL
);
";

/// Version 2 proves the seam: the two indexes the ported tests need, the
/// board's `(lane, position)` walk and the comment read's `(ticket_id)`.
const VERSION_TWO: &str = "
CREATE INDEX IF NOT EXISTS tickets_lane_position ON tickets (lane, position);
CREATE INDEX IF NOT EXISTS comments_ticket ON comments (ticket_id);
";

/// The version that adds `sessions.skills`, and the newest version a read of an
/// older file has to know about: the column is additive, so a file below this
/// version is readable with the column absent (TAN-241).
pub(crate) const SKILLS_VERSION: i64 = 3;

/// Version 3: the skill counts a session's pass loaded, stored as one JSON
/// object in a nullable column (TAN-241). **Additive**: an INSERT that names its
/// columns without `skills` leaves NULL, and the counts are absent on every
/// record written before the column existed. Absent is absent, never an empty
/// object.
///
/// Additive is not the same as backward compatible: a build that only knows v2
/// refuses the home this migration marks v3 (`refuse_legacy`), which is the
/// revert path a merge of this change has.
const VERSION_THREE: &str = "ALTER TABLE sessions ADD COLUMN skills TEXT;";

/// The version that adds the store's revision: one row in `store_meta`, moved
/// by every write that changes a row (PRT-11).
pub(crate) const REVISION_VERSION: i64 = 4;

/// Version 4: the store's revision, the number every listing carries and every
/// change event repeats.
///
/// It is a stored counter and not the engine's `data_version`, because the
/// revision has to mean the same thing to every process: `PRAGMA data_version`
/// is per connection and tells a reader only that *its* view moved. One row,
/// bumped inside the write's own transaction, is the one truth the CLI, the app
/// and a second terminal can agree on.
///
/// A read of a store below this version answers revision 0 and migrates
/// nothing (ADR 0031); the first write migrates and bumps.
const VERSION_FOUR: &str = "
CREATE TABLE IF NOT EXISTS store_meta (
    key   TEXT PRIMARY KEY,
    value INTEGER NOT NULL
);
INSERT OR IGNORE INTO store_meta (key, value) VALUES ('revision', 1);
";

/// The version that adds the session record's harness pointer (PRT-29).
pub(crate) const HARNESS_VERSION: i64 = 5;

/// Version 5: the harness session a record ran in, and the transport that
/// reported it (PRT-29).
///
/// **Additive, and the pointer, never the identity.** `sessions.id` stays
/// Tandem's own primary key; `harness_session_id` is the harness's id as a
/// foreign pointer, and `harness_transport` says how its usage is read. Both
/// are nullable: the hand-driven run names no harness, and a record written
/// before this version reads both as absent.
const VERSION_FIVE: &str = "
ALTER TABLE sessions ADD COLUMN harness_session_id TEXT;
ALTER TABLE sessions ADD COLUMN harness_transport TEXT;
";

/// The version that couples projects and lanes (TAN-295).
pub(crate) const PROJECTS_VERSION: i64 = 6;

/// Version 6: a `projects` table, and a parent on every lane.
///
/// `projects` holds `name` (primary key), `position` (render order, add
/// appends, no move verb) and `author`. `lanes` gains `project`: every lane's
/// exactly-one parent. The backfill makes one project per existing lane
/// (project name equals lane name, order follows lane order) and sets each
/// lane's parent; default-area tickets (no lane) are untouched.
///
/// The backfill copies the lane's author onto its project, so a migration
/// writes no author of its own: the project inherits the write that declared
/// its lane.
const VERSION_SIX: &str = "
CREATE TABLE IF NOT EXISTS projects (
    name     TEXT PRIMARY KEY,
    position INTEGER NOT NULL,
    author   TEXT NOT NULL
);
ALTER TABLE lanes ADD COLUMN project TEXT;
INSERT INTO projects (name, position, author)
    SELECT name, position, author FROM lanes ORDER BY position, rowid;
UPDATE lanes SET project = name WHERE project IS NULL;
";

/// The version that adds the workflow model (TAN-328).
pub(crate) const WORKFLOWS_VERSION: i64 = 7;

/// The default workflow's id: fixed, so the seed is one row at every home and
/// "the default" is a fact no user edit can rename away.
pub(crate) const DEFAULT_WORKFLOW_ID: &str = "00000000-0000-4000-8000-000000000001";

/// The default workflow's name.
pub const DEFAULT_WORKFLOW_NAME: &str = "agile";

/// The default workflow's stamps: the seed writes them, and the in-memory
/// fallback carries the same ones.
pub(crate) const DEFAULT_WORKFLOW_CREATED: &str = "2026-11-08T00:00:00Z";

/// One stage of the default `agile` workflow, as the v7 seed writes it and the
/// in-memory fallback builds it: key, label, role, and an optional binding
/// (roster role, pass).
#[derive(Clone, Copy)]
pub(crate) struct DefaultStage {
    pub key: &'static str,
    pub label: &'static str,
    pub role: &'static str,
    pub binding: Option<(&'static str, &'static str)>,
}

/// The v7 seed's six stages. **A shipped migration's data never changes**
/// (append only), so this list stays the foundation's six and the seventh
/// default stage lands in [`SELECTED_STAGE`] and the migration that adds it.
/// [`current_default_stages`] is the default today, and the in-memory fallback
/// reads it, so a fresh home and the fallback cannot disagree.
pub(crate) const DEFAULT_STAGES: [DefaultStage; 6] = [
    DefaultStage {
        key: "backlog",
        label: "Backlog",
        role: "intake",
        binding: Some(("refiner", "refine")),
    },
    DefaultStage {
        key: "on-hold",
        label: "On hold",
        role: "parked",
        binding: None,
    },
    DefaultStage {
        key: "ready",
        label: "Ready",
        role: "ready",
        binding: Some(("builder", "build")),
    },
    DefaultStage {
        key: "in-progress",
        label: "In progress",
        role: "active",
        binding: Some(("builder", "build")),
    },
    DefaultStage {
        key: "in-review",
        label: "In Review",
        role: "review",
        binding: Some(("reviewer", "review")),
    },
    DefaultStage {
        key: "done",
        label: "Done",
        role: "finished",
        binding: None,
    },
];

/// The default's second intake stage (TAN-327): between the pool and Ready, so
/// the human can promote the tickets the next refine pass should drain. It
/// carries no binding: refine reads the active workflow's intake stages by
/// role and order, never a second pass binding.
pub(crate) const SELECTED_STAGE: DefaultStage = DefaultStage {
    key: "selected",
    label: "Selected for development",
    role: "intake",
    binding: None,
};

/// The default `agile` workflow as this build reads it: the v7 six with
/// [`SELECTED_STAGE`] immediately before `ready`. A fresh home walks the v7
/// seed and then the selected-stage migration, so it ends here; the in-memory
/// fallback builds the same list, so a store with no workflow rows draws the
/// same default.
pub(crate) fn current_default_stages() -> Vec<DefaultStage> {
    let mut stages = DEFAULT_STAGES.to_vec();
    let at = stages
        .iter()
        .position(|stage| stage.key == "ready")
        .unwrap_or(stages.len());
    stages.insert(at, SELECTED_STAGE);
    stages
}

/// Version 7: the workflow entity, its ordered stages, and the nullable
/// `projects.workflow` hook for the later per-project adoption.
///
/// The default `agile` workflow seeds in [`version_seven`] from
/// [`DEFAULT_STAGES`], on a fresh home and on the v6 migration alike, because a
/// fresh home walks every migration. Its stage keys are the six the board
/// already draws, in UI-15's order: backlog, on-hold, ready, in-progress,
/// in-review, done. Every stored ticket status is one of those keys, so no
/// ticket moves.
const VERSION_SEVEN: &str = "
CREATE TABLE IF NOT EXISTS workflows (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    scope      TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    author     TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS workflow_stages (
    workflow_id  TEXT NOT NULL,
    position     INTEGER NOT NULL,
    key          TEXT NOT NULL,
    label        TEXT NOT NULL,
    role         TEXT NOT NULL,
    binding_role TEXT,
    binding_pass TEXT,
    PRIMARY KEY (workflow_id, key)
);
ALTER TABLE projects ADD COLUMN workflow TEXT;
";

fn version_one(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(VERSION_ONE)
}

fn version_two(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(VERSION_TWO)
}

fn version_three(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(VERSION_THREE)
}

fn version_four(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(VERSION_FOUR)
}

fn version_five(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(VERSION_FIVE)
}

fn version_six(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(VERSION_SIX)
}

/// The version that adds the session's turn and call counts (TAN-348).
pub(crate) const TURNS_VERSION: i64 = 8;

/// Version 8: how many assistant turns a session ran and how many tool calls
/// they made (TAN-348).
///
/// **Additive, and the pair is stamped together or not at all.** Both columns
/// are nullable: a record written before this version, or one with no harness
/// read, carries neither, never an invented zero. A row carrying one and not
/// the other is corrupt and refused on read.
const VERSION_EIGHT: &str = "
ALTER TABLE sessions ADD COLUMN turns INTEGER;
ALTER TABLE sessions ADD COLUMN calls INTEGER;
";

fn version_eight(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(VERSION_EIGHT)
}

/// The version that adds the default's second intake stage (TAN-327).
pub(crate) const SELECTED_VERSION: i64 = 9;

/// Version 9: `selected` into the default `agile` workflow, immediately before
/// `ready`.
///
/// **Only the untouched default is migrated.** The insert runs when the default
/// workflow's stage key set is exactly the six the v7 seed wrote and no
/// `selected` key exists; labels and positions do not block it, a different key
/// set does. A default that already carries `selected` is left alone, so the
/// migration is idempotent and a customized workflow is the human's, never
/// rewritten. The position shift and the insert share the migration's one
/// transaction.
fn version_nine(connection: &Connection) -> rusqlite::Result<()> {
    let mut statement = connection.prepare(
        "SELECT key, position FROM workflow_stages WHERE workflow_id = ?1 ORDER BY position",
    )?;
    let rows = statement
        .query_map([DEFAULT_WORKFLOW_ID], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    if rows.is_empty() {
        return Ok(());
    }
    let mut keys: Vec<&str> = rows.iter().map(|(key, _)| key.as_str()).collect();
    keys.sort_unstable();
    let mut seeded: Vec<&str> = DEFAULT_STAGES.iter().map(|stage| stage.key).collect();
    seeded.sort_unstable();
    if keys.contains(&SELECTED_STAGE.key) || keys != seeded {
        return Ok(());
    }

    let position = rows
        .iter()
        .find(|(key, _)| key == "ready")
        .map(|(_, position)| *position)
        .or_else(|| {
            rows.iter()
                .find(|(key, _)| key == "backlog")
                .map(|(_, position)| position + 1)
        })
        .unwrap_or(rows.len() as i64);

    connection.execute(
        "UPDATE workflow_stages SET position = position + 1
         WHERE workflow_id = ?1 AND position >= ?2",
        params![DEFAULT_WORKFLOW_ID, position],
    )?;
    connection.execute(
        "INSERT INTO workflow_stages
            (workflow_id, position, key, label, role, binding_role, binding_pass)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL)",
        params![
            DEFAULT_WORKFLOW_ID,
            position,
            SELECTED_STAGE.key,
            SELECTED_STAGE.label,
            SELECTED_STAGE.role
        ],
    )?;
    Ok(())
}

/// The version that turns `sprints` into `workflow_runs` (TAN-332).
pub(crate) const WORKFLOW_RUNS_VERSION: i64 = 10;

/// Version 10: the sprint record becomes the workflow's run record (TAN-332).
///
/// **The rename is in place.** A run keeps its `(lane, number)` identity and
/// every field, and gains the workflow it belongs to: the default `agile` id,
/// so every closed sprint on a real home migrates as a run of the default.
///
/// **The orphan sweep closes only what has no session behind it.** An open run
/// with `session IS NULL`, an empty batch and an empty run log was opened
/// without a pass, so nothing will ever close it; it closes at its own opening
/// stamp with the provenance below. An open run carrying a session, a ticket or
/// a run entry stays open.
const VERSION_TEN: &str = "
ALTER TABLE sprints RENAME TO workflow_runs;
ALTER TABLE workflow_runs ADD COLUMN workflow TEXT NOT NULL DEFAULT '{DEFAULT_WORKFLOW_ID}';
UPDATE workflow_runs
   SET status = 'closed',
       closed_at = opened_at,
       close = 'Closed by schema v10 migration: empty run opened with no session.'
 WHERE status = 'open'
   AND session IS NULL
   AND (batch IS NULL OR batch = '[]')
   AND (runs IS NULL OR runs = '[]');
";

/// [`VERSION_TEN`] with the default workflow's id filled in. A column default
/// cannot be bound as a parameter, so the one literal the DDL needs comes from
/// the constant rather than a second copy of the UUID.
fn version_ten_sql() -> String {
    VERSION_TEN.replace("{DEFAULT_WORKFLOW_ID}", DEFAULT_WORKFLOW_ID)
}

fn version_ten(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(&version_ten_sql())
}

fn version_seven(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(VERSION_SEVEN)?;
    connection.execute(
        "INSERT INTO workflows (id, name, scope, created_at, updated_at, author)
         VALUES (?1, ?2, 'home', ?3, ?3, 'human')",
        params![
            DEFAULT_WORKFLOW_ID,
            DEFAULT_WORKFLOW_NAME,
            DEFAULT_WORKFLOW_CREATED
        ],
    )?;
    for (position, stage) in DEFAULT_STAGES.iter().enumerate() {
        let (binding_role, binding_pass) = match stage.binding {
            Some((role, pass)) => (Some(role), Some(pass)),
            None => (None, None),
        };
        connection.execute(
            "INSERT INTO workflow_stages
                (workflow_id, position, key, label, role, binding_role, binding_pass)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                DEFAULT_WORKFLOW_ID,
                position as i64,
                stage.key,
                stage.label,
                stage.role,
                binding_role,
                binding_pass
            ],
        )?;
    }
    Ok(())
}

/// The store's revision: the stored counter, or 0 when this file predates the
/// table. A read never migrates, so an older file answers 0 rather than
/// failing on a table it does not have (ADR 0031).
pub(crate) fn revision(connection: &Connection) -> Result<i64> {
    if !has_table(connection, "store_meta")? {
        return Ok(0);
    }
    connection
        .query_row(
            "SELECT value FROM store_meta WHERE key = 'revision'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| translate(error, connection))
}

/// Move the revision by one. Called inside the write's own transaction, and
/// only when the write changed a row: a no-op is not a move.
pub(crate) fn bump_revision(connection: &Connection) -> Result<i64> {
    connection
        .execute(
            "UPDATE store_meta SET value = value + 1 WHERE key = 'revision'",
            [],
        )
        .map_err(|error| translate(error, connection))?;
    revision(connection)
}

/// Apply every migration newer than the recorded version, in order, one
/// transaction each. The version is read inside the transaction that records
/// it, so two writers opening a fresh home apply each migration once: the
/// second sees the first's row and applies nothing.
pub(crate) fn migrate(connection: &Connection, actor: &Actor) -> Result<()> {
    if CURRENT_VERSION != MIGRATIONS.len() as i64 {
        return Err(StoreError::Io(format!(
            "this build is schema v{CURRENT_VERSION} but ships {} migrations: the constant and \
             the list must agree",
            MIGRATIONS.len()
        )));
    }
    migrate_with(connection, MIGRATIONS, actor)
}

/// [`migrate`] over an explicit set, so the seam's own suite can prove the
/// order, the rollback and the upgrade without a second copy of the DDL.
pub(crate) fn migrate_with(
    connection: &Connection,
    migrations: &[Migration],
    actor: &Actor,
) -> Result<()> {
    require_contiguous(migrations)?;
    // A cheap pre-check so an up-to-date database pays no write transaction per
    // write. It only decides whether there is work: the version that counts is
    // read inside each migration's transaction below.
    if !needs_migration(connection)? {
        return Ok(());
    }
    for migration in migrations {
        let outcome = super::transaction(connection, || {
            create_seam(connection)?;
            let applied = version(connection)?.unwrap_or(0);
            if migration.version <= applied {
                return Ok(());
            }
            (migration.apply)(connection).map_err(|error| translate(error, connection))?;
            connection
                .execute(
                    "INSERT INTO schema_migrations (version, applied_at, author)
                     VALUES (?1, ?2, ?3)",
                    params![migration.version, timestamp::now(), actor.as_str()],
                )
                .map_err(|error| translate(error, connection))?;
            Ok(())
        });
        if let Err(error) = outcome {
            // Name the migration that failed; its transaction already rolled
            // back, so the store is never left half-migrated.
            return Err(match error {
                StoreError::Io(message) => StoreError::Io(format!(
                    "migration {} ({}) failed: {message}",
                    migration.version, migration.name
                )),
                other => other,
            });
        }
    }
    Ok(())
}

/// The highest applied migration, or `None` when the database has no seam yet.
pub(crate) fn version(connection: &Connection) -> Result<Option<i64>> {
    if !has_table(connection, "schema_migrations")? {
        return Ok(None);
    }
    connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .map_err(|error| translate(error, connection))
}

/// Whether a file is the Swift store's or newer than this build reads.
///
/// **The seam's actor is what tells the two stores apart.** The Swift store's
/// `schema_migrations` carries `version` and `appliedAt`; every version this
/// port writes carries its `author` (PRT-4), so a seam without that column is
/// the Swift store's, whatever its version says. The number alone cannot decide
/// it any more: the port's versions have moved past the Swift store's v5, so a
/// v5 file would otherwise read as merely behind this build and be migrated.
///
/// A Rust file whose seam is past [`CURRENT_VERSION`] is refused on the version,
/// as before: that is a newer build's store, and this one cannot read it.
pub fn refuse_legacy(connection: &Connection) -> Result<()> {
    if has_table(connection, "schema_migrations")?
        && !has_column(connection, "schema_migrations", "author")?
    {
        return Err(StoreError::Legacy {
            version: version(connection)?.unwrap_or(0),
        });
    }
    match version(connection)? {
        Some(applied) if applied > CURRENT_VERSION => Err(StoreError::Legacy { version: applied }),
        _ => Ok(()),
    }
}

/// Whether a write has to migrate before it runs.
pub(crate) fn behind(connection: &Connection) -> Result<bool> {
    match version(connection)? {
        Some(applied) => Ok(applied < CURRENT_VERSION),
        None => Ok(true),
    }
}

/// Whether the file carries the session record's skill column.
///
/// **A read asks this instead of migrating**, because a read never writes
/// (ADR 0031): a home the previous build wrote answers its records with the
/// additive column NULL, rather than failing on a column it does not have.
pub(crate) fn has_skills(connection: &Connection) -> Result<bool> {
    Ok(version(connection)?.unwrap_or(0) >= SKILLS_VERSION)
}

/// Whether the file carries the session record's harness columns (v5).
///
/// A read asks this rather than migrating: an older home answers both columns
/// NULL, and nothing is written (ADR 0031).
pub(crate) fn has_harness(connection: &Connection) -> Result<bool> {
    Ok(version(connection)?.unwrap_or(0) >= HARNESS_VERSION)
}

/// Whether the file carries the session record's turn and call columns (v8).
///
/// A read asks this rather than migrating: an older home answers both columns
/// NULL, and nothing is written (ADR 0031).
pub(crate) fn has_turns(connection: &Connection) -> Result<bool> {
    Ok(version(connection)?.unwrap_or(0) >= TURNS_VERSION)
}

/// Whether the file carries the workflow's run table (v10).
///
/// A read asks this rather than migrating (ADR 0031): a pre-v10 home answers its
/// run rows from `sprints`, with the default workflow supplied, and nothing is
/// written.
pub(crate) fn has_workflow_runs(connection: &Connection) -> Result<bool> {
    Ok(version(connection)?.unwrap_or(0) >= WORKFLOW_RUNS_VERSION)
}

/// Whether a store is behind the version this build writes.
fn needs_migration(connection: &Connection) -> Result<bool> {
    if !has_table(connection, "schema_migrations")? {
        return Ok(CURRENT_VERSION > 0);
    }
    Ok(version(connection)?.unwrap_or(0) < CURRENT_VERSION)
}

/// A migration list is exactly `1..=len`, in order: no hole, no renumbering.
///
/// **The seam walks a list, so a hole is a version nothing ever applied**, and
/// every later build would read a store whose shape skipped a step. The rule is
/// checked here rather than in a suite alone, because a suite only guards the
/// list this build ships; this guards every list, a test's own included.
fn require_contiguous(migrations: &[Migration]) -> Result<()> {
    for (index, migration) in migrations.iter().enumerate() {
        let expected = index as i64 + 1;
        if migration.version != expected {
            return Err(StoreError::Io(format!(
                "the migrations are not contiguous (version {} sits where {expected} belongs): \
                 a migration continues the count, it never leaves a hole",
                migration.version
            )));
        }
    }
    Ok(())
}

/// The seam's own table, created outside the versioned migrations because it is
/// the seam, not schema a version owns. The author is the actor of the write
/// that applied the migration: a migration is a write too.
fn create_seam(connection: &Connection) -> Result<()> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version    INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL,
                author     TEXT NOT NULL
            )",
        )
        .map_err(|error| translate(error, connection))
}

pub(crate) fn has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, column],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .map_err(|error| translate(error, connection))
}

pub(crate) fn has_table(connection: &Connection, name: &str) -> Result<bool> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .map_err(|error| translate(error, connection))
}
