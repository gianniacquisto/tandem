//! The store seam: where a run keeps its state, and the SQLite adapter that
//! lives there.
//!
//! One home everywhere, `~/.tandem` on macOS, Linux and Windows, with
//! `TANDEM_HOME` the only override and `--home` the run's own (docs/port.md,
//! Store). This module owns the home and the adapter; no SQL leaves it, and no
//! table shape reaches the wire.
//!
//! The store is one file, `<home>/tandem.db`, and the adapter is four rules:
//!
//! - **Schema v1, written once** (the `schema` module). The tables mirror today's v5
//!   tables as they are used. `schema_migrations` is the seam; a v5 file is
//!   refused with one error pointing at the one-off import's record (PRT-18,
//!   ADR 0031, the git history), because the port has no v5 reader and no dual
//!   writer.
//! - **An actor on every write** ([`Actor`], [`Write`]). Every mutation row
//!   carries an `author` column, and no listing infers authorship from a
//!   record.
//! - **One write, one transaction.** WAL, a ~2 s busy window that surfaces as
//!   [`StoreError::Locked`], and a rolling backup of the pre-write state
//!   (ADR 0018). Reads open read-only and never write.
//! - **A revision per write** (the `schema` module). `store_meta.revision`
//!   moves inside the write's transaction, so a listing and the event that
//!   follows it name the same number, and a receipt cannot carry a number a
//!   concurrent writer moved (PRT-11).
//! - **No sidecar lock.** The store takes no `flock` and writes no lock file:
//!   a `BEGIN IMMEDIATE` transaction is the writer and the ~2 s busy window is
//!   the contention bound across processes (ADR 0031), so a `tandem.lock`
//!   beside a home is inert. Two threads of one process queue on a process-local
//!   gate (one mutex per canonical home, no file), and a thread that cannot
//!   enter it in the same ~2 s window answers "locked", so in-process
//!   contention is bounded exactly as the engine bounds it across processes.
//!   The atomic file helper here (temp naming, commit, directory sync, orphan
//!   sweep) is shared by the rolling backup and by the bikeshed's locked
//!   writes; the bikeshed's sidecar lock lives in [`crate::bikeshed`].

pub(crate) mod board_rows;
mod diagnostics;
mod home;
// Reachable from the schema suite, which builds a v2 home to prove a read of
// an older file never fails on a column a later migration adds.
pub(crate) mod schema;
#[cfg(test)]
mod tests;
mod write;

use std::collections::HashMap;
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, ErrorCode, OpenFlags, backup::Backup};

pub use diagnostics::{BackupHealth, CommentsHealth, CommentsState, Health, comments_state};
pub use home::{ENVIRONMENT, Home, HomeSource, user_home};
pub use schema::CURRENT_VERSION;
pub use write::{NewComment, NewDependency, NewLane, NewProject, NewTicket, Write};

use write::BackupState;

/// The environment variable that names the actor of an agent-run write.
pub const ACTOR_ENVIRONMENT: &str = "TANDEM_AGENT";

/// Who is making a write. `human` is the app's face and an agent's unset
/// environment; a pass name (`builder`, `reviewer`, …) is the agent's.
///
/// The value comes from the place the harness already sets: `TANDEM_AGENT` is
/// the pass's name when an agent runs a verb, and unset (or blank) means the
/// human. One truth, so every verb takes the same variable and no verb grows an
/// `--actor` flag (PRT-4, Decided against).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// The human, written as `human`.
    Human,
    /// An agent pass, written as the pass's name.
    Agent(String),
}

impl Actor {
    /// The pure precedence rule, so it can be tested without setting a
    /// process's environment: a non-blank value is the pass, anything else the
    /// human. Surrounding whitespace is not part of a name.
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).filter(|name| !name.is_empty()) {
            Some(name) => Self::Agent(name.to_string()),
            None => Self::Human,
        }
    }

    /// The actor a process runs as: [`Actor::parse`] over `TANDEM_AGENT`.
    /// The CLI's write verbs call this once per run and hand it to
    /// [`Store::write`]; no verb grows an `--actor` flag.
    pub fn current() -> Self {
        Self::parse(env::var(ACTOR_ENVIRONMENT).ok().as_deref())
    }

    /// The word a mutation row records.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Human => "human",
            Self::Agent(name) => name,
        }
    }
}

impl fmt::Display for Actor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

// Timestamps on the wire and in the store: ISO-8601 UTC, whole seconds. The
// Swift store writes `2026-10-30T12:27:28Z` and reads it back
// (`Timestamp.swift`), and the port keeps the same text because the schema
// mirrors the tables as they are used. No date library: the store needs an
// encoder for "now" and a decoder for the rows it reads, and both are the
// civil-date arithmetic below.

/// The current instant, whole seconds, as the store writes it.
pub(crate) fn now() -> String {
    encode(unix_now())
}

/// The current instant as whole seconds since the Unix epoch.
pub(crate) fn unix_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs() as i64,
        Err(_) => 0,
    }
}

/// `seconds` since the Unix epoch as `YYYY-MM-DDTHH:MM:SSZ`.
pub(crate) fn encode(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        of_day / 3_600,
        (of_day % 3_600) / 60,
        of_day % 60
    )
}

/// The instant `text` names, or `None` when it is not `YYYY-MM-DDTHH:MM:SSZ`.
/// A row the store cannot read is an error at its reader, never a silent zero.
///
/// The read side is the domain's: the record aggregates decode a row's stamps
/// and derive the span between them (PRT-6), so a row the store cannot read
/// fails at its reader rather than at a later render.
pub(crate) fn decode(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }
    let year: i64 = text[0..4].parse().ok()?;
    let month: u32 = text[5..7].parse().ok()?;
    let day: u32 = text[8..10].parse().ok()?;
    let hour: i64 = text[11..13].parse().ok()?;
    let minute: i64 = text[14..16].parse().ok()?;
    let second: i64 = text[17..19].parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// The civil date `days` since the epoch names (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let of_era = shifted - era * 146_097;
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

/// The day count `year-month-day` names since the epoch, the inverse of
/// [`civil_from_days`].
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_prime = if month > 2 { month - 3 } else { month + 9 } as i64;
    let day_of_year = (153 * month_prime + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The contention window the store's protocol promised (ADR 0001): a writer
/// waits ~2 s for another process's transaction, then the write fails as
/// "locked".
const BUSY_TIMEOUT: Duration = Duration::from_millis(2_000);

/// The process-local write gate: one mutex per canonical home.
///
/// Across processes, SQLite's `BEGIN IMMEDIATE` and the busy window are the
/// contention bound (ADR 0031). Inside one process that is not enough: two
/// threads that reach the engine together race the busy handler instead of
/// queueing, and on the Windows runner a writer answered `Locked` after the
/// full window while its sibling held the write for a few milliseconds. The
/// gate makes those writers queue, deterministically and with no file at all:
/// it is keyed on the canonical home, it lives only in this process, and a
/// stale `tandem.lock` has nothing to do with it.
fn write_gate(home: &Path) -> Arc<Gate> {
    static GATES: OnceLock<Mutex<HashMap<PathBuf, Arc<Gate>>>> = OnceLock::new();
    let gates = GATES.get_or_init(|| Mutex::new(HashMap::new()));
    let key = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    let mut gates = gates
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(gates.entry(key).or_default())
}

/// The process-local write gate: the lock one home's writers queue on.
///
/// The flag is the lock and the condition variable is the wait: a writer that
/// finds the gate held sleeps until the holder drops its guard, and wakes once
/// instead of retrying on `yield_now` for the length of the window.
struct Gate {
    held: Mutex<bool>,
    free: Condvar,
    /// Test-facing: how many times a writer passed through the wait loop. A
    /// condvar wait spends one or two; a `yield_now` retry spends thousands in
    /// the same window.
    #[cfg(test)]
    wait_passes: std::sync::atomic::AtomicUsize,
}

impl Default for Gate {
    fn default() -> Self {
        Self {
            held: Mutex::new(false),
            free: Condvar::new(),
            #[cfg(test)]
            wait_passes: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

/// The gate held: it clears the flag and wakes the next writer when it drops.
struct GateGuard<'a> {
    gate: &'a Gate,
}

impl Drop for GateGuard<'_> {
    fn drop(&mut self) {
        let mut held = self
            .gate
            .held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *held = false;
        drop(held);
        self.gate.free.notify_one();
    }
}

/// Enter the gate within the store's window, or answer the store's one word
/// for contention.
///
/// The gate is bounded by [`BUSY_TIMEOUT`], the same window the engine gives a
/// writer in another process: the sidecar's 2 s window used to bound
/// in-process contention too, and a thread that cannot enter in that window
/// gets [`StoreError::Locked`] rather than waiting for the writer ahead of it
/// to finish. The holder is a local thread mid-write, and its work is bounded
/// by the same window it was given.
///
/// The wait is a `Condvar`, not a retry loop: the waiter parks until the holder
/// notifies it or the window closes, so contention costs the waiting thread no
/// CPU. A notified waiter whose window closed in the meantime answers locked,
/// exactly as a `yield_now` loop would have.
fn enter_gate<'a>(gate: &'a Gate, home: &Path, timeout: Duration) -> Result<GateGuard<'a>> {
    let deadline = Instant::now() + timeout;
    let mut held = gate
        .held
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    loop {
        #[cfg(test)]
        gate.wait_passes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if !*held {
            *held = true;
            drop(held);
            return Ok(GateGuard { gate });
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Err(StoreError::Locked {
                home: home.to_path_buf(),
            });
        };
        let (woken, outcome) = gate
            .free
            .wait_timeout(held, remaining)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        held = woken;
        if outcome.timed_out() && *held {
            return Err(StoreError::Locked {
                home: home.to_path_buf(),
            });
        }
    }
}

/// What a store failure is, in the store's words. Every engine failure that is
/// not contention becomes [`StoreError::Io`] with the engine's detail, exactly
/// as the Swift store translates `SQLiteError` (ADR 0007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The gate or the write transaction was held past the window. The home
    /// names the store the caller chose, because a sibling thread of this
    /// process answers the same words as a writer in another one.
    Locked { home: PathBuf },
    /// Anything else: an unreadable file, a full disk, a failed migration.
    Io(String),
    /// A file this build does not read: schema newer than [`CURRENT_VERSION`].
    Legacy { version: i64 },
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locked { home } => write!(
                formatter,
                "the store at {} is locked: another writer held it past the 2 s window",
                home.display()
            ),
            Self::Io(message) => formatter.write_str(message),
            Self::Legacy { version } => write!(
                formatter,
                "this home's tandem.db is schema v{version}, which this build does not read; \
                 the one-off import of a v5 home is recorded in ADR 0031 and the git history (PRT-18)"
            ),
        }
    }
}

impl std::error::Error for StoreError {}

/// The store's result: [`StoreError`], never a bare engine error.
pub type Result<T> = std::result::Result<T, StoreError>;

/// The store rooted at one home. Cheap to clone, safe to share: every
/// operation opens its own connection, so the store holds no lock.
#[derive(Debug, Clone)]
pub struct Store {
    home: Home,
    /// How long this store's writes wait for the process-local gate: the
    /// production [`BUSY_TIMEOUT`], unless a test configures its own. The
    /// constant stays 2 s; only the contention cases widen their own store.
    gate_timeout: Duration,
    /// The revision of the last write this store performed, read inside that
    /// write's transaction and shared by every clone. A receipt can carry the
    /// exact number its rows are at, without a second transaction that another
    /// writer could move.
    last_revision: Arc<AtomicI64>,
    /// How many times `watch` has opened a connection. Test-facing: the point
    /// of the watch is that a held connection is reused, not reopened a tick.
    /// Every platform reads it: the restore's unix case replaces the store
    /// under the held watch, and its cross-platform case attaches to the
    /// rollback-journal file without renaming over a held one (PRT-26).
    #[cfg(test)]
    watch_opens: Arc<std::sync::atomic::AtomicU64>,
    /// How many times a held watch has read the revision. Test-facing: the
    /// file watch must not query on every tick, so an idle case pins that
    /// this stays put while the 40 ms cadence stats the files.
    #[cfg(test)]
    watch_revision_reads: Arc<std::sync::atomic::AtomicU64>,
}

impl Store {
    pub fn new(home: Home) -> Self {
        Self {
            home,
            gate_timeout: BUSY_TIMEOUT,
            last_revision: Arc::new(AtomicI64::new(0)),
            #[cfg(test)]
            watch_opens: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            #[cfg(test)]
            watch_revision_reads: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Test-facing: this store with its own gate window. The production
    /// [`BUSY_TIMEOUT`] stays 2 s; a contention case on a loaded runner
    /// configures its own, and a deliberate hold longer than that configured
    /// window still answers `Locked`.
    #[cfg(test)]
    pub(crate) fn with_gate_timeout(mut self, timeout: Duration) -> Self {
        self.gate_timeout = timeout;
        self
    }

    pub fn home(&self) -> &Home {
        &self.home
    }

    /// Whether the home carries a database file. The watch asks this to tell a
    /// home nothing has written yet, whose retry costs a `stat` and no
    /// connection, from a store that exists but carries no revision row (a
    /// pre-v4 file), which every attempt opens.
    pub fn exists(&self) -> bool {
        self.database().exists()
    }

    /// The database: `<home>/tandem.db`.
    pub fn database(&self) -> PathBuf {
        self.home.directory().join("tandem.db")
    }

    /// The rolling backup: `<home>/tandem.db.bak`, one generation (ADR 0018).
    pub fn backup(&self) -> PathBuf {
        self.home.directory().join("tandem.db.bak")
    }

    /// The write-ahead log the engine appends to between checkpoints:
    /// `<home>/tandem.db-wal`. Absent in rollback-journal mode and after a
    /// checkpoint, so the file watch treats a missing file as a state of its
    /// own rather than an error.
    pub fn wal(&self) -> PathBuf {
        self.home.directory().join("tandem.db-wal")
    }

    /// What the file watch stats on every tick: the database and its WAL, each
    /// as its size and modification time. Two stats, no connection and no
    /// query, so an idle child costs nothing a human would notice. A missing
    /// file is `None`, because a fresh home has no database yet and a
    /// checkpointed store has no WAL.
    pub fn watch_files(&self) -> WatchFiles {
        WatchFiles {
            db: FileSig::of(&self.database()),
            wal: FileSig::of(&self.wal()),
        }
    }

    /// A read: read-only, lock-free, and never writing. `None` when there is no
    /// database yet, so a fresh home reads as empty rather than as an error and
    /// a read creates no directory and no file.
    ///
    /// Internal: the connection it hands back speaks SQL, and all SQL lives in
    /// this module. The typed readers PRT-5 and PRT-6 build on it are the
    /// public surface, so no caller outside the adapter can write a query
    /// (docs/port.md, Store).
    pub(crate) fn read<T>(&self, body: impl FnOnce(&Connection) -> Result<T>) -> Result<Option<T>> {
        let database = self.database();
        if !database.exists() {
            return Ok(None);
        }
        let connection = open_read_only(&database)?;
        // A v5 file is refused on the way in, before any statement reads a
        // table the port does not have.
        schema::refuse_legacy(&connection)?;
        body(&connection).map(Some)
    }

    /// A read and the revision it is a snapshot of, in **one** read
    /// transaction: the first `SELECT` takes the snapshot, the second sees the
    /// same one, so a listing cannot pair rows with a revision a writer moved
    /// between two statements.
    pub(crate) fn read_snapshot<T>(
        &self,
        body: impl FnOnce(&Connection) -> Result<T>,
    ) -> Result<Option<(T, i64)>> {
        let database = self.database();
        if !database.exists() {
            return Ok(None);
        }
        let connection = open_read_only(&database)?;
        schema::refuse_legacy(&connection)?;
        connection
            .execute_batch("BEGIN DEFERRED")
            .map_err(|error| translate(error, &connection))?;
        let outcome = (|| {
            let revision = schema::revision(&connection)?;
            let value = body(&connection)?;
            Ok((value, revision))
        })();
        match outcome {
            Ok(value) => {
                connection
                    .execute_batch("COMMIT")
                    .map_err(|error| translate(error, &connection))?;
                Ok(Some(value))
            }
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// A read-only connection kept open for the change watch. The file watch
    /// stats the database and its WAL on every tick and reads the revision
    /// through this only when the files move (or the 5 s fallback fires), so
    /// an idle child stats twice a tick instead of querying ~25 times a
    /// second.
    ///
    /// **Any readable store is watchable, WAL or not.** ADR 0031's restore
    /// renames the rollback-journal backup over `tandem.db`; refusing a
    /// non-WAL file would leave the stream blind until the next write switched
    /// WAL back, and the app showing the pre-restore board. The revision lives
    /// in `store_meta`, which reads in either journal mode, so the watch
    /// attaches and reports the restored number. A file with no revision row
    /// yet (a fresh home mid-first-write, or a pre-v4 file) answers `None`; the
    /// caller retries, and holding a reader there would only race the first
    /// write's one-time WAL switch.
    pub fn watch(&self) -> Result<Option<RevisionWatch>> {
        let database = self.database();
        if !database.exists() {
            return Ok(None);
        }
        #[cfg(test)]
        self.watch_opens.fetch_add(1, Ordering::Relaxed);
        let connection = open_read_only(&database)?;
        schema::refuse_legacy(&connection)?;
        if schema::revision(&connection)? == 0 {
            return Ok(None);
        }
        let file = StoreFile::of(&database)?;
        Ok(Some(RevisionWatch {
            connection,
            database,
            file,
            #[cfg(test)]
            reads: Arc::clone(&self.watch_revision_reads),
        }))
    }

    /// How many connections `watch` has opened. Test-facing, so a case can pin
    /// that a held watch is reused rather than reopened on every tick.
    #[cfg(test)]
    pub(crate) fn watch_opens(&self) -> u64 {
        self.watch_opens.load(Ordering::Relaxed)
    }

    /// How many revision reads a held watch has issued. Test-facing, so an
    /// idle case can pin that the 40 ms file-stat cadence costs no query.
    #[cfg(test)]
    pub(crate) fn watch_revision_reads(&self) -> u64 {
        self.watch_revision_reads.load(Ordering::Relaxed)
    }

    /// A write: one `BEGIN IMMEDIATE` transaction with the caller's inserts
    /// recorded under `actor`, and no sidecar lock: the engine's write lock and
    /// the busy window are the contention bound across processes, and the
    /// process-local gate queues this process's own threads within the same
    /// window (ADR 0031).
    pub fn write<T>(&self, actor: &Actor, body: impl FnOnce(&Write<'_>) -> Result<T>) -> Result<T> {
        self.home.ensure().map_err(|error| {
            StoreError::Io(format!(
                "could not create the home {}: {error}",
                self.home.directory().display()
            ))
        })?;
        // Two threads of this process queue here, each turned away as "locked"
        // after the store's window; across processes the engine's write lock
        // and the busy window are the bound (ADR 0031).
        let gate = write_gate(self.home.directory());
        let _gate = enter_gate(&gate, self.home.directory(), self.gate_timeout)?;
        // A writer killed mid-backup leaves its temp beside the database, and
        // nothing renames it: the sweep under the gate clears it, the same rule
        // the bikeshed's vault applies to its notes' temps (ADR 0031).
        sweep_orphans(self.home.directory());
        let database = self.database();
        let existed = database.exists();
        let connection = open(&database)?;
        // Refuse before any table is written and before the journal mode
        // moves. A clean v5 file is left byte for byte; one carrying a hot
        // journal has that journal rolled back by this open itself, which
        // changes no row and no schema.
        schema::refuse_legacy(&connection)?;
        switch_to_wal(&connection)?;
        // A migration commits in its own transaction, so the pre-migration
        // image is rolled here, before it runs: rolled afterwards, `.bak`
        // would already hold the new version and a previous build would
        // have nothing to restore.
        let migrating = existed && schema::behind(&connection)?;
        if migrating {
            backup_database(&database, &self.backup())?;
        }
        schema::migrate(&connection, actor)?;

        begin_immediate(&connection)?;
        let changes_before = connection.total_changes();
        let write = Write {
            connection: &connection,
            actor,
            backup: BackupState {
                database: &database,
                target: self.backup(),
                armed: existed && !migrating,
                taken: std::cell::Cell::new(false),
            },
        };
        match body(&write) {
            Ok(value) => {
                // The revision moves with the rows, inside the same
                // transaction, and is read back before the commit: the receipt
                // carries the number its own rows are at, never a later one a
                // second transaction could pair with them. The UPDATE statements
                // guard on the values, so a write that touched a row without
                // changing it (a same-value set) is not a change event.
                let revision = if connection.total_changes() > changes_before {
                    schema::bump_revision(&connection)?
                } else {
                    schema::revision(&connection)?
                };
                commit(&connection)?;
                self.last_revision.store(revision, Ordering::Relaxed);
                Ok(value)
            }
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// The applied migration version, or `None` when there is no database yet.
    /// For `doctor` (ADR 0014): the seam reported as a fact, never migrated.
    pub fn schema_version(&self) -> Result<Option<i64>> {
        match self.read(schema::version)? {
            Some(version) => Ok(version),
            None => Ok(None),
        }
    }

    /// The revision of the last write this store performed, read inside that
    /// write's transaction. A receipt carries this number, so its payload and
    /// its revision cannot be split by a concurrent writer.
    pub fn last_revision(&self) -> i64 {
        self.last_revision.load(Ordering::Relaxed)
    }

    /// The store's revision: the number every listing carries and every change
    /// event repeats (PRT-11).
    ///
    /// A home with no database yet reads 0, and a file written before the
    /// revision existed reads 0 too: a read migrates nothing (ADR 0031). The
    /// number moves inside the write's transaction, so a reader either sees the
    /// row change and the revision together or sees neither.
    pub fn revision(&self) -> Result<i64> {
        Ok(self.read(schema::revision)?.unwrap_or(0))
    }
}

/// A read-only connection the change watch keeps open and reads only when
/// the files move.
///
/// ADR 0031's restore replaces `tandem.db` by rename, so a connection held for
/// the process's life would keep reading the retired inode after a restore and
/// the stream would go silent. The watch therefore remembers the file it opened
/// and answers an error once the path names another one; the caller reopens.
pub struct RevisionWatch {
    connection: Connection,
    database: PathBuf,
    file: StoreFile,
    /// Test-facing: how many revision reads this held connection has issued,
    /// shared with the `Store` that opened it so an idle case can read the
    /// count off the store.
    #[cfg(test)]
    reads: Arc<std::sync::atomic::AtomicU64>,
}

impl RevisionWatch {
    /// The store's revision as this connection currently sees it.
    pub fn revision(&self) -> Result<i64> {
        if StoreFile::of(&self.database)? != self.file {
            return Err(StoreError::Io(format!(
                "{} was replaced; the watch reopens",
                self.database.display()
            )));
        }
        #[cfg(test)]
        self.reads.fetch_add(1, Ordering::Relaxed);
        schema::revision(&self.connection)
    }
}

/// One file the watch stats: its size and its modification time. Two stats
/// and no query, so the 40 ms cadence is cheap enough to run all day. A write
/// moves one of the two (the WAL grows, a checkpoint moves the database), and
/// a coarse filesystem that reports neither is what the 5 s fallback is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSig {
    len: u64,
    modified: Option<std::time::SystemTime>,
}

impl FileSig {
    fn of(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
}

/// What the file watch compares on every tick: the database and its WAL. The
/// revision is read only when this moves (or the fallback fires), so an idle
/// app stats twice a tick and queries nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchFiles {
    db: Option<FileSig>,
    wal: Option<FileSig>,
}

/// The identity of the file a watch holds, enough to tell that the path now
/// names a different file without mistaking a write for a replacement.
/// `dev`/`ino` decide on Unix; elsewhere the creation time does, because a
/// renamed-over database is a different file and a write never recreates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StoreFile {
    created: Option<std::time::SystemTime>,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}

impl StoreFile {
    fn of(path: &Path) -> Result<Self> {
        let metadata = fs::metadata(path).map_err(|error| {
            StoreError::Io(format!("could not stat {}: {error}", path.display()))
        })?;
        Ok(Self {
            created: metadata.created().ok(),
            #[cfg(unix)]
            dev: std::os::unix::fs::MetadataExt::dev(&metadata),
            #[cfg(unix)]
            ino: std::os::unix::fs::MetadataExt::ino(&metadata),
        })
    }
}

/// The store's temp prefix: its own files only, so the sweep cannot touch the
/// bikeshed's.
const TEMP_PREFIX: &str = ".tandem-";

const TEMP_SUFFIX: &str = ".tmp";

/// A temp older than this is a crash's leftover, never an in-flight write's.
const ORPHAN_MAX_AGE: Duration = Duration::from_secs(60);

/// A temp path beside `target`, unique across processes and calls.
pub(crate) fn sibling(target: &Path, prefix: &str) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{prefix}{}{TEMP_SUFFIX}", stamp()))
}

/// A name no other writer in this process or another can pick at the same
/// nanosecond: the process id, the wall clock and a counter.
fn stamp() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = NEXT.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}-{serial}", std::process::id())
}

/// Make `temp` durable, run `before_commit`, and rename it over `target`.
///
/// `before_commit` runs once the new bytes are on disk but **before** the
/// rename: the seam for work that must see the file the write is about to
/// replace (the store's rolling backup). The old file is still in place when it
/// runs, and still in place if it throws, so neither failure can leave a
/// half-written file. A failed commit removes its temp.
pub(crate) fn commit_file(
    temp: &Path,
    target: &Path,
    before_commit: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let outcome = (|| {
        // The sync handle carries write access: Windows' `FlushFileBuffers`
        // needs `GENERIC_WRITE` and answers "Access is denied" on the read-only
        // handle `File::open` would give, and that is every commit, on every
        // write, since the backup rides this path.
        let file = fs::OpenOptions::new()
            .write(true)
            .open(temp)
            .map_err(|error| {
                StoreError::Io(format!(
                    "could not open the temp at {}: {error}",
                    temp.display()
                ))
            })?;
        file.sync_all().map_err(|error| {
            StoreError::Io(format!(
                "could not fsync the temp at {}: {error}",
                temp.display()
            ))
        })?;
        before_commit()?;
        fs::rename(temp, target).map_err(|error| {
            StoreError::Io(format!(
                "could not rename {} to {}: {error}",
                temp.display(),
                target.display()
            ))
        })
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(temp);
    }
    outcome
}

/// Flush the directory's entries: a rename is durable only once the entry it
/// changed is synced. An unopenable directory is a failed guarantee, not a
/// tolerated absence, so the caller cannot proceed unconfirmed.
#[cfg(unix)]
pub(crate) fn fsync_directory(root: &Path) -> Result<()> {
    let directory = fs::File::open(root).map_err(|error| {
        StoreError::Io(format!(
            "could not open directory {}: {error}",
            root.display()
        ))
    })?;
    directory.sync_all().map_err(|error| {
        StoreError::Io(format!(
            "could not fsync directory {}: {error}",
            root.display()
        ))
    })
}

/// Windows has no directory fsync; the rename is still the atomic step.
#[cfg(not(unix))]
pub(crate) fn fsync_directory(_root: &Path) -> Result<()> {
    Ok(())
}

/// Remove `<prefix>*<suffix>` files older than [`ORPHAN_MAX_AGE`] from one flat
/// directory: the store's backup temps sit beside the database.
///
/// The store calls this itself, under its own gate, before it opens the
/// database: a writer killed mid-backup leaves its temp and nothing renames it,
/// so the next write is what clears it.
pub(crate) fn sweep_orphans(directory: &Path) {
    let cutoff = SystemTime::now()
        .checked_sub(ORPHAN_MAX_AGE)
        .unwrap_or(UNIX_EPOCH);
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(TEMP_PREFIX) || !name.ends_with(TEMP_SUFFIX) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if modified < cutoff {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// The rolling backup: copy the last committed state into a same-directory
/// temp through SQLite's own backup API, close it, `fsync`, and rename it over
/// `tandem.db.bak`. A copy that cannot be taken fails the write; a copy that
/// fails leaves the previous backup untouched.
fn backup_database(database: &Path, target: &Path) -> Result<()> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        StoreError::Io(format!(
            "could not create the backup's directory {}: {error}",
            parent.display()
        ))
    })?;
    let temp = sibling(target, ".tandem-backup-");
    let outcome = copy_into(database, &temp).and_then(|()| {
        commit_file(&temp, target, || Ok(())).map_err(|error| {
            StoreError::Io(format!(
                "could not roll the backup into place at {}: {error}",
                target.display()
            ))
        })?;
        // The rename is durable only once the directory entry it changed is
        // synced, and the backup is the generation a restore trusts.
        fsync_directory(parent)
    });
    if outcome.is_err() {
        let _ = fs::remove_file(&temp);
    }
    outcome
}

/// The copy half of [`backup_database`]: the engine's one complete generation,
/// put back in rollback-journal mode so the one-file backup opens without a
/// `-shm` beside it.
fn copy_into(database: &Path, temp: &Path) -> Result<()> {
    let source = open_read_only(database)?;
    let mut destination = Connection::open(temp).map_err(|error| {
        StoreError::Io(format!(
            "could not open the backup at {}: {error}",
            temp.display()
        ))
    })?;
    {
        let copy = Backup::new_with_names(
            &source,
            rusqlite::MAIN_DB,
            &mut destination,
            rusqlite::MAIN_DB,
        )
        .map_err(|error| StoreError::Io(format!("could not start the backup: {error}")))?;
        copy.run_to_completion(100, Duration::ZERO, None)
            .map_err(|error| StoreError::Io(format!("could not copy the database: {error}")))?;
    }
    destination
        .execute_batch("PRAGMA journal_mode=DELETE")
        .map_err(|error| {
            StoreError::Io(format!("could not make the backup a single file: {error}"))
        })?;
    // The engine's handles must be gone and the bytes on disk before the temp
    // is moved into place, or the moved file can read as an empty database.
    drop(destination);
    Ok(())
}

/// A read-write connection with the store's busy window.
fn open(path: &Path) -> Result<Connection> {
    let connection = Connection::open(path)
        .map_err(|error| StoreError::Io(format!("could not open {}: {error}", path.display())))?;
    connection.busy_timeout(BUSY_TIMEOUT).map_err(|error| {
        StoreError::Io(format!(
            "could not set the busy window on {}: {error}",
            path.display()
        ))
    })?;
    Ok(connection)
}

/// A read-only connection: a read never writes, and never creates a file.
fn open_read_only(path: &Path) -> Result<Connection> {
    let connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|error| {
            StoreError::Io(format!(
                "could not open {} read-only: {error}",
                path.display()
            ))
        })?;
    connection.busy_timeout(BUSY_TIMEOUT).map_err(|error| {
        StoreError::Io(format!(
            "could not set the busy window on {}: {error}",
            path.display()
        ))
    })?;
    Ok(connection)
}

/// Turn WAL on when the file is not already in it.
///
/// `PRAGMA journal_mode=WAL` is not a read: it takes the write lock and the
/// busy handler does not cover it, so re-asserting it on every open loses a
/// write the moment another connection holds the lock (the concurrency suite
/// caught that in the Swift store). Read the mode, switch only when it is not
/// `wal`, and wait out the one contended case (two connections creating the
/// same home) inside the store's own window.
fn switch_to_wal(connection: &Connection) -> Result<()> {
    if journal_mode(connection)?.is_some_and(|mode| mode.eq_ignore_ascii_case("wal")) {
        return Ok(());
    }
    let deadline = std::time::Instant::now() + BUSY_TIMEOUT;
    loop {
        match connection.execute_batch("PRAGMA journal_mode=WAL") {
            Ok(()) => return Ok(()),
            Err(error) if is_busy(&error) => {
                if journal_mode(connection)?.is_some_and(|mode| mode.eq_ignore_ascii_case("wal")) {
                    return Ok(());
                }
                if std::time::Instant::now() >= deadline {
                    return Err(translate(error, connection));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(translate(error, connection)),
        }
    }
}

fn journal_mode(connection: &Connection) -> Result<Option<String>> {
    connection
        .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
        .map(Some)
        .map_err(|error| translate(error, connection))
}

/// One transaction: the atomic write the port keeps from ADR 0031. `IMMEDIATE`
/// takes the write lock up front, so a read-modify-write cannot interleave
/// with another writer. A throw rolls everything back; nothing half-applies.
pub(crate) fn transaction<T>(
    connection: &Connection,
    body: impl FnOnce() -> Result<T>,
) -> Result<T> {
    begin_immediate(connection)?;
    match body() {
        Ok(value) => {
            commit(connection)?;
            Ok(value)
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn begin_immediate(connection: &Connection) -> Result<()> {
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| translate(error, connection))
}

fn commit(connection: &Connection) -> Result<()> {
    connection
        .execute_batch("COMMIT")
        .map_err(|error| translate(error, connection))
}

/// The engine's contention becomes the store's "locked"; every other failure
/// the store's I/O words with the engine's detail (ADR 0007's translation
/// rule, at the new seam).
pub(crate) fn translate(error: rusqlite::Error, connection: &Connection) -> StoreError {
    if is_busy(&error) {
        StoreError::Locked {
            home: home_of(connection),
        }
    } else {
        StoreError::Io(error.to_string())
    }
}

/// The home a connection's database lives in. Every connection the store opens
/// sits directly at `<home>/tandem.db`, so the parent names the store a
/// contended write failed on. An in-memory connection has no path, and no
/// store path leads there.
fn home_of(connection: &Connection) -> PathBuf {
    connection
        .path()
        .and_then(|path| Path::new(path).parent())
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(
                failure.code,
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
            )
    )
}
