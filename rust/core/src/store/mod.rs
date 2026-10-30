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
//! - **The write discipline** (the `locked` module): the `flock` sidecar with bounded
//!   retry, same-directory temps with `fsync` + rename, and the orphan sweep.
//!   A lock that is not held is a lock; a temp older than a minute is a
//!   crash's.

mod actor;
mod home;
mod locked;
mod rows;
mod schema;
#[cfg(test)]
mod tests;
mod timestamp;
mod write;

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, ErrorCode, OpenFlags, backup::Backup};

pub use actor::{Actor, ENVIRONMENT as ACTOR_ENVIRONMENT};
pub use home::{ENVIRONMENT, Home, HomeSource, user_home};
pub use rows::{NewComment, NewDependency, NewLane, NewSession, NewSetting, NewSprint, NewTicket};
pub use schema::CURRENT_VERSION;
pub use write::Write;

use locked::LockedDirectory;
use write::BackupState;

/// The contention window the store's protocol promised (ADR 0001): a writer
/// waits ~2 s for another process's transaction, then the write fails as
/// "locked".
const BUSY_TIMEOUT: Duration = Duration::from_millis(2_000);

/// What a store failure is, in the store's words. Every engine failure that is
/// not contention becomes [`StoreError::Io`] with the engine's detail, exactly
/// as the Swift store translates `SQLiteError` (ADR 0007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The sidecar lock or the write transaction was held past the window.
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
}

impl Store {
    pub fn new(home: Home) -> Self {
        Self { home }
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

    /// A write: the sidecar lock, one `BEGIN IMMEDIATE` transaction, and the
    /// caller's inserts recorded under `actor`.
    pub fn write<T>(&self, actor: &Actor, body: impl FnOnce(&Write<'_>) -> Result<T>) -> Result<T> {
        self.home.ensure().map_err(|error| {
            StoreError::Io(format!(
                "could not create the home {}: {error}",
                self.home.directory().display()
            ))
        })?;
        let locked = LockedDirectory::for_store(self.home.directory());
        locked.with_lock(|| {
            let database = self.database();
            let existed = database.exists();
            let connection = open(&database)?;
            // Refuse before the journal mode moves: a v5 file is left as it was.
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
                    commit(&connection)?;
                    Ok(value)
                }
                Err(error) => {
                    let _ = connection.execute_batch("ROLLBACK");
                    Err(error)
                }
            }
        })
    }

    /// The applied migration version, or `None` when there is no database yet.
    /// For `doctor` (ADR 0014): the seam reported as a fact, never migrated.
    pub fn schema_version(&self) -> Result<Option<i64>> {
        match self.read(schema::version)? {
            Some(version) => Ok(version),
            None => Ok(None),
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
