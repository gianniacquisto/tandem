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
//!   refused with one error naming `Scripts/import-legacy.py` (PRT-18), because
//!   the port has no v5 reader and no dual writer.
//! - **An actor on every write** ([`Actor`], [`Write`]). Every mutation row
//!   carries an `author` column, and no listing infers authorship from a
//!   session row.
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
//!   gate (one mutex per canonical home, no file), so the engine only arbitrates
//!   across processes. The `locked` module keeps the sidecar, the same-directory
//!   temp with `fsync` + rename and the orphan sweep for the bikeshed; the store
//!   borrows only the temp, the commit and the directory sync for the rolling
//!   backup.

mod actor;
pub(crate) mod board_rows;
mod home;
pub(crate) mod locked;
pub(crate) mod records;
mod rows;
// Reachable from the records suite, which builds a v2 home to prove a read of
// an older file never fails on a column a later migration adds.
pub(crate) mod schema;
#[cfg(test)]
mod tests;
pub(crate) mod timestamp;
mod write;

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OpenFlags, backup::Backup};

pub use actor::{Actor, ENVIRONMENT as ACTOR_ENVIRONMENT};
pub use home::{ENVIRONMENT, Home, HomeSource, user_home};
pub use records::local_day;
pub use rows::{
    NewComment, NewDependency, NewLane, NewSession, NewSetting, NewSprint, NewTicket, SessionRow,
    SprintRow,
};
pub use schema::CURRENT_VERSION;
pub use write::Write;

use locked::LockedDirectory;
use write::BackupState;

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
fn write_gate(home: &Path) -> Arc<Mutex<()>> {
    static GATES: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    let gates = GATES.get_or_init(|| Mutex::new(HashMap::new()));
    let key = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    let mut gates = gates
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(gates.entry(key).or_default())
}

/// What a store failure is, in the store's words. Every engine failure that is
/// not contention becomes [`StoreError::Io`] with the engine's detail, exactly
/// as the Swift store translates `SQLiteError` (ADR 0007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The write transaction was held past the window.
    Locked,
    /// Anything else: an unreadable file, a full disk, a failed migration.
    Io(String),
    /// A file this build does not read: schema newer than [`CURRENT_VERSION`].
    Legacy { version: i64 },
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locked => formatter
                .write_str("the store is locked: another writer held it past the 2 s window"),
            Self::Io(message) => formatter.write_str(message),
            Self::Legacy { version } => write!(
                formatter,
                "this home's tandem.db is schema v{version}, which this build does not read; \
                 import it once with Scripts/import-legacy.py (PRT-18)"
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
    /// The revision of the last write this store performed, read inside that
    /// write's transaction and shared by every clone. A receipt can carry the
    /// exact number its rows are at, without a second transaction that another
    /// writer could move.
    last_revision: Arc<AtomicI64>,
    /// How many times `watch` has opened a connection. Test-facing: the point
    /// of the watch is that a held connection is reused, not reopened a tick.
    #[cfg(test)]
    watch_opens: Arc<std::sync::atomic::AtomicU64>,
}

impl Store {
    pub fn new(home: Home) -> Self {
        Self {
            home,
            last_revision: Arc::new(AtomicI64::new(0)),
            #[cfg(test)]
            watch_opens: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    pub fn home(&self) -> &Home {
        &self.home
    }

    /// The database: `<home>/tandem.db`.
    pub fn database(&self) -> PathBuf {
        self.home.directory().join("tandem.db")
    }

    /// The rolling backup: `<home>/tandem.db.bak`, one generation (ADR 0018).
    pub fn backup(&self) -> PathBuf {
        self.home.directory().join("tandem.db.bak")
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
            .map_err(translate)?;
        let outcome = (|| {
            let revision = schema::revision(&connection)?;
            let value = body(&connection)?;
            Ok((value, revision))
        })();
        match outcome {
            Ok(value) => {
                connection.execute_batch("COMMIT").map_err(translate)?;
                Ok(Some(value))
            }
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// A read-only connection kept open for the change watch. The watch polls
    /// this instead of opening a connection per tick: an idle child runs one
    /// query on one connection, not ~25 opens a second.
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
        }))
    }

    /// How many connections `watch` has opened. Test-facing, so a case can pin
    /// that a held watch is reused rather than reopened on every tick.
    #[cfg(test)]
    pub(crate) fn watch_opens(&self) -> u64 {
        self.watch_opens.load(Ordering::Relaxed)
    }

    /// A write: one `BEGIN IMMEDIATE` transaction with the caller's inserts
    /// recorded under `actor`, and no sidecar lock: the engine's write lock and
    /// the busy window are the contention bound (ADR 0031).
    pub fn write<T>(&self, actor: &Actor, body: impl FnOnce(&Write<'_>) -> Result<T>) -> Result<T> {
        self.home.ensure().map_err(|error| {
            StoreError::Io(format!(
                "could not create the home {}: {error}",
                self.home.directory().display()
            ))
        })?;
        // Two threads of this process queue here; across processes the engine's
        // write lock and the busy window are the bound (ADR 0031).
        let gate = write_gate(self.home.directory());
        let _gate = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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

    /// Every session row, in stored order. An empty result is a successful
    /// empty list, and a home with no database yet reads empty rather than
    /// failing: the first write creates the store.
    pub fn sessions(&self) -> Result<Vec<SessionRow>> {
        Ok(self.read(records::session_rows)?.unwrap_or_default())
    }

    /// Every sprint row, in stored order.
    pub fn sprints(&self) -> Result<Vec<SprintRow>> {
        Ok(self.read(records::sprint_rows)?.unwrap_or_default())
    }

    /// Whether the board carries a ticket id, archive included. A read, never a
    /// write: the sprint's batch check asks the store, it does not touch it.
    pub fn carries_ticket(&self, id: &str) -> Result<bool> {
        Ok(self
            .read(|connection| records::ticket_carried(connection, id))?
            .unwrap_or(false))
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

/// A read-only connection the change watch keeps open and polls.
///
/// ADR 0031's restore replaces `tandem.db` by rename, so a connection held for
/// the process's life would keep reading the retired inode after a restore and
/// the stream would go silent. The watch therefore remembers the file it opened
/// and answers an error once the path names another one; the caller reopens.
pub struct RevisionWatch {
    connection: Connection,
    database: PathBuf,
    file: StoreFile,
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
        schema::revision(&self.connection)
    }
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
    let temp = locked::sibling(target, ".tandem-backup-");
    let storage = LockedDirectory::for_store(parent);
    let outcome = copy_into(database, &temp).and_then(|()| {
        storage.commit(&temp, target, || Ok(())).map_err(|error| {
            StoreError::Io(format!(
                "could not roll the backup into place at {}: {error}",
                target.display()
            ))
        })?;
        // The rename is durable only once the directory entry it changed is
        // synced, and the backup is the generation a restore trusts.
        storage.fsync_directory()
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
                    return Err(translate(error));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(translate(error)),
        }
    }
}

fn journal_mode(connection: &Connection) -> Result<Option<String>> {
    connection
        .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
        .map(Some)
        .map_err(translate)
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
        .map_err(translate)
}

fn commit(connection: &Connection) -> Result<()> {
    connection.execute_batch("COMMIT").map_err(translate)
}

/// The engine's contention becomes the store's "locked"; every other failure
/// the store's I/O words with the engine's detail (ADR 0007's translation
/// rule, at the new seam).
pub(crate) fn translate(error: rusqlite::Error) -> StoreError {
    if is_busy(&error) {
        StoreError::Locked
    } else {
        StoreError::Io(error.to_string())
    }
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
