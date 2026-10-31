//! The store's ported suite: the write discipline (ADR 0001, ADR 0007), the
//! SQLite seam (ADR 0031), the rolling backup (ADR 0018), the actor on every
//! write, and the v5 refusal.
//!
//! Ported from `LockedDirectoryTests`, `RecoveryTests`, `ConcurrencyTests`,
//! `StoreDocumentsTests` and `SQLiteSeamTests`: the behaviours those suites
//! assert, not their Swift spelling. Like the Swift seam suite, this one is
//! allowed to know the storage; everything above it talks to `Store` only.

use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use rusqlite::{Connection, OpenFlags};

use super::*;

static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

/// A throwaway home: `root/home` is the home, `root/user` a sandbox user home,
/// so "the user's `~/.tandem`" is a path no run may touch unless it resolved
/// the default.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let serial = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-store-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the sandbox is creatable");
        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn home_path(&self) -> PathBuf {
        self.root.join("home")
    }

    fn user_home(&self) -> PathBuf {
        self.root.join("user")
    }

    fn home(&self) -> Home {
        Home::resolve(Some(&self.home_path()), None, &self.user_home())
    }

    fn store(&self) -> Store {
        Store::new(self.home())
    }

    fn database(&self) -> PathBuf {
        self.home_path().join("tandem.db")
    }

    fn backup(&self) -> PathBuf {
        self.home_path().join("tandem.db.bak")
    }

    /// A read-only connection for the assertions that need the storage itself.
    fn connection(&self) -> Connection {
        Connection::open_with_flags(self.database(), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("the database opens")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn setting(store: &Store, actor: &Actor, key: &str, value: &str) {
    store
        .write(actor, |write| {
            write.insert_setting(NewSetting { key, value })
        })
        .expect("the write lands");
}

/// Backdate a file, so the orphan sweep reads it as a crash's leftover rather
/// than as a write in flight.
fn aged(path: &Path, seconds: u64) {
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .expect("the temp opens");
    let when = SystemTime::now() - Duration::from_secs(seconds);
    file.set_modified(when).expect("the temp's age is set");
}

fn author_of(connection: &Connection, table: &str) -> Vec<String> {
    let sql = format!("SELECT author FROM {table} ORDER BY rowid");
    let mut statement = connection.prepare(&sql).expect("the table answers");
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("the rows read")
        .map(|row| row.expect("a row reads"))
        .collect()
}

fn has_index(connection: &Connection, name: &str) -> bool {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            [name],
            |row| row.get::<_, i64>(0),
        )
        .expect("the schema reads")
        > 0
}

/// Every table whose inserts carry the actor, and its author column. The seam's
/// own table is in the list: a migration is a write too.
const AUTHORED_TABLES: [&str; 8] = [
    "tickets",
    "ticket_dependencies",
    "comments",
    "sessions",
    "sprints",
    "lanes",
    "settings",
    "schema_migrations",
];

// MARK: - The write discipline (LockedDirectory)

mod locked {
    use std::fs::{self, OpenOptions};

    use super::*;

    fn storage(sandbox: &Sandbox) -> super::super::locked::LockedDirectory {
        fs::create_dir_all(sandbox.root()).expect("the root is creatable");
        super::super::locked::LockedDirectory::for_store(sandbox.root())
    }

    #[test]
    fn atomic_write_replaces_the_file_and_leaves_no_temp() {
        let sandbox = Sandbox::new("atomic");
        let storage = storage(&sandbox);
        let file = sandbox.root().join("data.txt");

        storage
            .atomic_write(b"one", &file)
            .expect("the first write lands");
        assert_eq!(fs::read(&file).unwrap(), b"one");
        storage
            .atomic_write(b"two", &file)
            .expect("the second write lands");
        assert_eq!(fs::read(&file).unwrap(), b"two");

        let leftovers: Vec<String> = fs::read_dir(sandbox.root())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".tandem-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
    }

    #[test]
    fn atomic_write_creates_missing_parents() {
        let sandbox = Sandbox::new("parents");
        let storage = storage(&sandbox);
        let file = sandbox.root().join("a").join("b").join("c.txt");

        storage
            .atomic_write(b"deep", &file)
            .expect("the write creates its parents");

        assert_eq!(fs::read(&file).unwrap(), b"deep");
    }

    #[test]
    fn orphan_sweep_removes_stale_temps_and_keeps_fresh_ones() {
        let sandbox = Sandbox::new("orphans");
        let storage = storage(&sandbox);
        let stale = sandbox.root().join(".tandem-stale.tmp");
        fs::write(&stale, b"stale").unwrap();
        aged(&stale, 120);
        let fresh = sandbox.root().join(".tandem-fresh.tmp");
        fs::write(&fresh, b"fresh").unwrap();

        let _guard = storage.lock().expect("the lock runs");

        assert!(!stale.exists(), "the stale orphan was not removed");
        assert!(fresh.exists(), "the fresh temp was wrongly removed");
    }

    #[test]
    fn the_orphan_sweep_stays_out_of_subdirectories() {
        let sandbox = Sandbox::new("orphans-nested");
        let storage = storage(&sandbox);
        let nested = sandbox.root().join("bikeshed").join(".tandem-old.tmp");
        fs::create_dir_all(nested.parent().unwrap()).unwrap();
        fs::write(&nested, b"stale").unwrap();
        aged(&nested, 120);

        let _guard = storage.lock().expect("the lock runs");

        assert!(
            nested.exists(),
            "the flat sweep must not descend into the bikeshed's temps"
        );
    }

    #[test]
    fn lock_holds_the_exclusive_lock_for_the_guard() {
        let sandbox = Sandbox::new("excludes");
        let storage = storage(&sandbox);
        let lock = sandbox.root().join("tandem.lock");

        let guard = storage.lock().expect("the lock is taken");
        let held = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock)
            .expect("the lock file opens");
        assert!(
            matches!(held.try_lock(), Err(fs::TryLockError::WouldBlock)),
            "a second handle took the lock while the guard held it"
        );
        drop(guard);

        let free = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock)
            .expect("the lock file opens");
        assert!(
            free.try_lock().is_ok(),
            "the lock was not released after the guard"
        );
    }

    #[test]
    fn a_held_lock_times_out_as_locked() {
        let sandbox = Sandbox::new("timeout");
        let storage = storage(&sandbox);
        let lock = sandbox.root().join("tandem.lock");
        let held = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock)
            .expect("the lock file opens");
        held.try_lock().expect("the test takes the lock");

        let started = Instant::now();
        let outcome = storage.lock().map(|_guard| ());
        let elapsed = started.elapsed();
        drop(held);

        assert!(
            matches!(outcome, Err(StoreError::Locked { .. })),
            "the bounded retry must answer locked: {outcome:?}"
        );
        assert!(
            elapsed >= Duration::from_millis(1_500),
            "the retry window is the store's ~2 s, waited {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(15),
            "the retry did not wait far past the window: {elapsed:?}"
        );
    }

    /// A nested acquisition is a body that takes the same lock file again: it
    /// queues behind itself, so it must answer `Locked` at the window like any
    /// other writer that cannot be served, never hang. The wait is bounded on
    /// both halves (`Queue::take` and the `flock` retry), so this case bounds
    /// the test at three windows instead of relying on the suite's timeout.
    #[test]
    fn a_nested_acquisition_answers_locked_at_the_window_instead_of_hanging() {
        let sandbox = Sandbox::new("nested");
        let storage = storage(&sandbox);
        let (answered, waiting) = std::sync::mpsc::channel();
        // Detached, not scoped: a scoped thread that hangs takes the test with
        // it, and the point of this case is the answer.
        std::thread::spawn({
            let storage = storage.clone();
            move || {
                let started = Instant::now();
                let outcome = storage
                    .lock()
                    .and_then(|_held| storage.lock().map(|_nested| ()));
                let _ = answered.send((
                    matches!(outcome, Err(StoreError::Locked { .. })),
                    started.elapsed(),
                ));
            }
        });

        let (locked, elapsed) = waiting
            .recv_timeout(super::super::locked::LOCK_TIMEOUT * 3)
            .expect("a nested acquisition answers at the window, it does not hang");
        assert!(
            locked,
            "a nested acquisition answers Locked, not Ok: it took {elapsed:?}"
        );
        assert!(
            elapsed >= super::super::locked::LOCK_TIMEOUT,
            "the nested acquisition answered before the window: {elapsed:?}"
        );
    }

    /// Two spellings of one lock file are one queue, or the fairness guarantee
    /// is lost for writers that spell their home differently (a symlink, a `.`
    /// or `..` segment, a different case on Windows): both would take ticket 0
    /// and race the `flock` again.
    #[test]
    fn two_spellings_of_one_home_share_one_queue() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let sandbox = Sandbox::new("spellings");
        let root = sandbox.root();
        let plain = root.join("tandem.lock");
        let dotted = root.join(".").join("tandem.lock");
        assert_eq!(
            super::super::locked::queue_key(&plain).expect("the plain spelling canonicalizes"),
            super::super::locked::queue_key(&dotted).expect("the dotted spelling canonicalizes"),
            "two spellings of one lock file must key one queue"
        );

        // And they exclude each other: the dotted spelling waits out a holder
        // on the plain one instead of taking a second lock.
        let holder =
            super::super::locked::LockedDirectory::new(plain, ".tandem-", root.to_path_buf());
        let waiter =
            super::super::locked::LockedDirectory::new(dotted, ".tandem-", root.to_path_buf());
        let holding = Arc::new(Barrier::new(2));
        let held_for = Duration::from_millis(300);
        thread::scope(|scope| {
            scope.spawn(|| {
                let _held = holder.lock().expect("the holder's body runs");
                holding.wait();
                thread::sleep(held_for);
            });
            holding.wait();

            let started = Instant::now();
            let _served = waiter
                .lock()
                .expect("a writer on the other spelling is served, not locked out");
            assert!(
                started.elapsed() >= held_for,
                "the waiter did not wait for the holder: {:?}",
                started.elapsed()
            );
        });
    }

    /// A writer that arrives while another holds the lock is served in arrival
    /// order: two waiters queue behind the holder, and the queue hands them the
    /// lock in the order they took their tickets. This is the property `Queue`
    /// exists for; without it both waiters spin `try_lock` and the order is a
    /// coin flip per release. The case waits on the queue's own ticket count,
    /// so a bypassed `Queue::take` fails here (the count never moves) instead
    /// of passing by luck.
    #[test]
    fn waiters_are_served_in_arrival_order() {
        let sandbox = Sandbox::new("order");
        let storage = storage(&sandbox);
        let key = super::super::locked::queue_key(&sandbox.root().join("tandem.lock"))
            .expect("the sandbox root canonicalizes");

        let held = storage.lock().expect("the holder takes the lock");
        // The holder's own ticket is taken; every later arrival adds one.
        let holder_ticket = super::super::locked::queued(&key);
        let (served, order) = std::sync::mpsc::channel();
        let mut waiters = Vec::new();
        for name in ["first", "second"] {
            let storage = storage.clone();
            let served = served.clone();
            waiters.push(std::thread::spawn(move || {
                let _guard = storage.lock().expect("the waiter is served");
                let _ = served.send(name);
            }));
            // Wait until this waiter has taken its ticket: that is its arrival,
            // and it is what the queue orders on. A sleep would assume the
            // thread ran; the count is the synchronization point.
            let deadline = Instant::now() + super::super::locked::LOCK_TIMEOUT;
            let wanted = holder_ticket + waiters.len() as u64;
            while super::super::locked::queued(&key) < wanted {
                assert!(
                    Instant::now() < deadline,
                    "waiter {} did not take a queue ticket; the queue is bypassed",
                    waiters.len()
                );
                std::thread::yield_now();
            }
        }
        drop(held);

        let arrivals: Vec<&str> = (0..2)
            .map(|_| {
                order
                    .recv_timeout(super::super::locked::LOCK_TIMEOUT * 3)
                    .expect("both waiters report")
            })
            .collect();
        for waiter in waiters {
            waiter.join().expect("the waiter thread joins");
        }
        assert_eq!(
            arrivals,
            ["first", "second"],
            "waiters are served in the order they queued"
        );
    }

    /// A lock file whose directory cannot canonicalize is refused, not keyed on
    /// the path as spelled: the spelled path is the split this key exists to
    /// close, and two spellings of one home would each get a queue. A missing
    /// directory is the portable way to make `canonicalize` fail.
    #[test]
    fn a_directory_that_cannot_canonicalize_is_refused() {
        let sandbox = Sandbox::new("uncanonical");
        let missing = sandbox.root().join("missing").join("tandem.lock");

        let outcome = super::super::locked::queue_key(&missing);

        assert!(
            matches!(&outcome, Err(StoreError::Io(_))),
            "a directory that cannot canonicalize is refused: {outcome:?}"
        );
    }

    #[test]
    fn an_ordered_write_keeps_the_old_file_until_the_commit() {
        let sandbox = Sandbox::new("ordered");
        let storage = storage(&sandbox);
        let target = sandbox.root().join("tandem.db.bak");
        fs::write(&target, b"old").unwrap();
        let temp = sandbox.root().join(".tandem-ordered.tmp");
        fs::write(&temp, b"new").unwrap();

        storage
            .commit(&temp, &target, || {
                assert_eq!(
                    fs::read(&target).unwrap(),
                    b"old",
                    "the old file is still in place before the rename"
                );
                Ok(())
            })
            .expect("the commit lands");

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!temp.exists(), "the commit left its temp behind");
    }

    #[test]
    fn a_failed_commit_leaves_the_old_file_and_no_temp() {
        let sandbox = Sandbox::new("failed-commit");
        let storage = storage(&sandbox);
        let target = sandbox.root().join("tandem.db.bak");
        fs::write(&target, b"old").unwrap();
        let temp = sandbox.root().join(".tandem-failed.tmp");
        fs::write(&temp, b"new").unwrap();

        let outcome = storage.commit(&temp, &target, || {
            Err(StoreError::Io("the write changed its mind".to_string()))
        });

        assert!(outcome.is_err(), "a throwing commit fails");
        assert_eq!(fs::read(&target).unwrap(), b"old", "the old file survived");
        assert!(!temp.exists(), "the failed commit left its temp behind");
    }

    #[cfg(unix)]
    #[test]
    fn a_commit_syncs_a_temp_it_cannot_open_for_reading() {
        use std::os::unix::fs::PermissionsExt;

        let sandbox = Sandbox::new("write-only-temp");
        let storage = storage(&sandbox);
        let target = sandbox.root().join("tandem.db.bak");
        fs::write(&target, b"old").unwrap();
        let temp = sandbox.root().join(".tandem-write-only.tmp");
        fs::write(&temp, b"new").unwrap();
        // Windows' `FlushFileBuffers` needs a handle carrying write access, so a
        // commit that opens its temp read-only cannot sync it there. A temp with
        // no read permission is the same requirement, provable on unix: the
        // commit must sync through a handle opened for writing.
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o200)).unwrap();

        storage
            .commit(&temp, &target, || Ok(()))
            .expect("the commit syncs a write-only temp and lands");

        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_fsync_over_an_unopenable_directory_fails() {
        let sandbox = Sandbox::new("fsync");
        // The root sits behind a regular file, so open(2) fails with ENOTDIR.
        let not_a_directory = sandbox.root().join("not-a-dir");
        fs::write(&not_a_directory, b"a file").unwrap();
        let storage = super::super::locked::LockedDirectory::new(
            sandbox.root().join("tandem.lock"),
            ".tandem-",
            not_a_directory.join("sub"),
        );

        let outcome = storage.fsync_directory();

        match outcome {
            Err(StoreError::Io(message)) => {
                assert!(
                    message.contains("open directory"),
                    "the failure names the open: {message}"
                );
            }
            other => panic!("expected a directory open failure, got {other:?}"),
        }
    }

    #[test]
    fn a_directory_fsync_succeeds_over_a_real_directory() {
        let sandbox = Sandbox::new("fsync-ok");
        let storage = storage(&sandbox);
        storage.fsync_directory().expect("a real directory syncs");
    }
}

// MARK: - The in-process write gate

mod gate {
    use std::sync::{Arc, mpsc};
    use std::thread;

    use super::*;

    #[test]
    fn one_home_is_one_gate() {
        let sandbox = Sandbox::new("gate-key");
        let home = sandbox.home_path();
        fs::create_dir_all(&home).unwrap();

        assert!(
            Arc::ptr_eq(&write_gate(&home), &write_gate(&home)),
            "the same home must answer the same gate"
        );
        assert!(
            Arc::ptr_eq(&write_gate(&home), &write_gate(&home.join("."))),
            "another spelling of the home must answer the same gate"
        );
    }

    #[test]
    fn in_process_writers_queue_on_the_gate() {
        let sandbox = Sandbox::new("gate-queue");
        let home = sandbox.home_path();
        fs::create_dir_all(&home).unwrap();
        let gate = write_gate(&home);

        let held = enter_gate(&gate, &home).expect("the gate is taken");
        let (tx, entered) = mpsc::channel();
        let queued = {
            let gate = Arc::clone(&gate);
            let home = home.clone();
            thread::spawn(move || {
                let _guard = enter_gate(&gate, &home).expect("the queued writer takes the gate");
                tx.send(()).expect("the parent is listening");
            })
        };

        assert!(
            entered.recv_timeout(Duration::from_millis(150)).is_err(),
            "a second writer entered while the first held the gate"
        );
        drop(held);
        entered
            .recv_timeout(Duration::from_secs(5))
            .expect("the queued writer runs once the gate is free");
        queued.join().expect("the queued writer finishes");
    }

    /// Contention must block, not burn a core: the waiter sleeps on the gate's
    /// condition variable and wakes once the holder releases it, so a 150 ms
    /// hold costs the waiter one pass through the wait instead of the
    /// thousands a `yield_now` loop spends.
    #[test]
    fn a_contended_writer_blocks_on_the_gate_instead_of_spinning() {
        let sandbox = Sandbox::new("gate-condvar");
        let home = sandbox.home_path();
        fs::create_dir_all(&home).unwrap();
        let gate = write_gate(&home);
        let held = enter_gate(&gate, &home).expect("the test takes the gate");
        gate.wait_passes.store(0, Ordering::Relaxed);

        let (tx, answered) = mpsc::channel();
        let waiting = {
            let gate = Arc::clone(&gate);
            let home = home.clone();
            thread::spawn(move || {
                let started = Instant::now();
                let outcome = enter_gate(&gate, &home).map(|_guard| ());
                let elapsed = started.elapsed();
                let passes = gate.wait_passes.load(Ordering::Relaxed);
                let _ = tx.send((outcome, elapsed, passes));
            })
        };

        thread::sleep(Duration::from_millis(150));
        drop(held);
        let (outcome, elapsed, passes) = answered
            .recv_timeout(Duration::from_secs(5))
            .expect("the queued writer answers once the gate is free");
        waiting.join().expect("the queued writer finishes");

        assert!(
            outcome.is_ok(),
            "the queued writer entered the gate: {outcome:?}"
        );
        assert!(
            elapsed >= Duration::from_millis(100),
            "the queued writer waited behind the holder: {elapsed:?}"
        );
        assert!(
            passes <= 3,
            "the wait slept on the gate's condition variable, not a yield loop: \
             {passes} passes in {elapsed:?}"
        );
    }

    #[test]
    fn a_held_gate_times_out_as_locked() {
        let sandbox = Sandbox::new("gate-deadline");
        let home = sandbox.home_path();
        fs::create_dir_all(&home).unwrap();
        let gate = write_gate(&home);
        let held = enter_gate(&gate, &home).expect("the test takes the gate");

        // The engine is free while this thread holds the gate, so only the
        // gate's own deadline can turn the queued writer away.
        let store = sandbox.store();
        let (tx, answered) = mpsc::channel();
        let queued = thread::spawn(move || {
            let started = Instant::now();
            let outcome = store.write(&Actor::Human, |write| {
                write.insert_setting(NewSetting {
                    key: "blocked",
                    value: "1",
                })
            });
            tx.send((outcome, started.elapsed()))
                .expect("the parent is listening");
        });

        let (outcome, elapsed) = answered
            .recv_timeout(Duration::from_secs(15))
            .expect("a held gate must answer within the store's window");
        drop(held);
        queued.join().expect("the queued writer finishes");

        assert!(
            matches!(&outcome, Err(StoreError::Locked { home: named }) if named == &home),
            "a held gate must time out as locked, naming the home: {outcome:?}"
        );
        assert!(
            elapsed >= Duration::from_millis(1_500),
            "the writer waited the store's window (elapsed {elapsed:?})"
        );
        assert!(
            elapsed < Duration::from_secs(15),
            "the writer did not wait far past the window (elapsed {elapsed:?})"
        );
        assert!(
            !sandbox.database().exists(),
            "a write turned away at the gate opened no database"
        );
    }
}

// MARK: - The SQLite seam (schema, migrations, backup, contention)

mod schema {
    use super::super::schema::{CURRENT_VERSION, MIGRATIONS, Migration, migrate_with, version};
    use super::*;

    fn good(connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch("CREATE TABLE good (x INTEGER)")
    }

    fn bad(connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch("CREATE TABLE bad (x INTEGER)")?;
        // The migration fails after its first statement: the transaction must
        // take the created table back with it.
        connection.execute_batch("INSERT INTO missing_table (x) VALUES (1)")
    }

    #[test]
    fn the_current_version_is_the_last_migration() {
        assert_eq!(
            CURRENT_VERSION,
            MIGRATIONS
                .last()
                .expect("the store ships a migration")
                .version,
            "the build's version and the last migration cannot disagree"
        );
    }

    /// The list is contiguous: version `n` sits at index `n - 1`, and the
    /// build's constant is the count. A hole is caught here, by this build's own
    /// suite, before it can be caught by a home that skipped a step.
    #[test]
    fn the_migration_list_is_contiguous() {
        let versions: Vec<i64> = MIGRATIONS
            .iter()
            .map(|migration| migration.version)
            .collect();
        assert_eq!(
            versions,
            (1..=CURRENT_VERSION).collect::<Vec<_>>(),
            "every version from 1 to CURRENT_VERSION, in order"
        );
        assert_eq!(
            MIGRATIONS.len() as i64,
            CURRENT_VERSION,
            "the constant is the migration count"
        );
    }

    /// And a list with a hole is refused by the seam itself, not only by the
    /// assertion above: any caller, a suite's own list included, gets named.
    #[test]
    fn a_migration_list_with_a_hole_is_refused() {
        let sandbox = Sandbox::new("migrate-hole");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        let migrations = [
            Migration {
                version: 1,
                name: "the store",
                apply: good,
            },
            Migration {
                version: 3,
                name: "after the hole",
                apply: good,
            },
        ];

        match migrate_with(&connection, &migrations, &Actor::Human) {
            Err(StoreError::Io(message)) => assert!(
                message.contains("not contiguous"),
                "the refusal names the rule: {message}"
            ),
            other => panic!("a hole must fail the open, got {other:?}"),
        }
        assert_eq!(
            version(&connection).unwrap(),
            None,
            "nothing is applied before the list is refused"
        );
    }

    #[test]
    fn a_fresh_homes_first_write_creates_the_database_at_the_current_version() {
        let sandbox = Sandbox::new("fresh");
        let store = sandbox.store();

        setting(&store, &Actor::Human, "defaultLane", "Port");

        assert!(
            sandbox.database().exists(),
            "the first write creates tandem.db"
        );
        assert_eq!(store.schema_version().unwrap(), Some(CURRENT_VERSION));
        let connection = sandbox.connection();
        let mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal", "readers stay lock-free in WAL");
        let mut statement = connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap();
        let versions: Vec<i64> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(versions, (1..=CURRENT_VERSION).collect::<Vec<_>>());
        let applied_at: String = connection
            .query_row(
                "SELECT applied_at FROM schema_migrations LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            super::super::timestamp::decode(&applied_at).is_some(),
            "applied_at is an ISO-8601 UTC timestamp: {applied_at}"
        );
    }

    #[test]
    fn the_tables_are_the_ones_the_format_names() {
        let sandbox = Sandbox::new("tables");
        setting(&sandbox.store(), &Actor::Human, "k", "v");

        let connection = sandbox.connection();
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .unwrap();
        let tables: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();

        for table in [
            "schema_migrations",
            "tickets",
            "ticket_dependencies",
            "comments",
            "sessions",
            "sprints",
            "lanes",
            "settings",
        ] {
            assert!(tables.iter().any(|name| name == table), "missing {table}");
        }
    }

    #[test]
    fn a_read_creates_nothing() {
        let sandbox = Sandbox::new("read");
        let store = sandbox.store();

        let contents = store.read(|_| Ok(())).expect("a read answers");

        assert!(contents.is_none(), "a fresh home has no database to read");
        assert!(
            !sandbox.home_path().exists(),
            "a read creates no home and no database"
        );
    }

    #[test]
    fn migrations_apply_in_order_and_record_their_version() {
        let sandbox = Sandbox::new("migrations");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        let migrations = [
            Migration {
                version: 1,
                name: "one",
                apply: good,
            },
            Migration {
                version: 2,
                name: "two",
                apply: |connection| connection.execute_batch("CREATE TABLE two (x INTEGER)"),
            },
        ];

        migrate_with(&connection, &migrations, &Actor::Human).expect("the migrations apply");

        assert_eq!(version(&connection).unwrap(), Some(2));
        let mut statement = connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap();
        let versions: Vec<i64> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(versions, vec![1, 2]);
        for applied_at in connection
            .prepare("SELECT applied_at FROM schema_migrations")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
        {
            assert!(
                super::super::timestamp::decode(&applied_at.unwrap()).is_some(),
                "every applied migration carries an ISO-8601 UTC timestamp"
            );
        }
    }

    #[test]
    fn a_second_migrate_applies_nothing() {
        let sandbox = Sandbox::new("migrate-twice");
        let store = sandbox.store();
        setting(&store, &Actor::Human, "k", "v");

        let connection = sandbox.connection();
        migrate_with(&connection, MIGRATIONS, &Actor::Human).expect("a no-op migrate answers");

        assert_eq!(version(&connection).unwrap(), Some(CURRENT_VERSION));
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, CURRENT_VERSION, "each migration is recorded once");
    }

    #[test]
    fn a_failing_migration_rolls_back_and_fails_the_open() {
        let sandbox = Sandbox::new("migrate-fails");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        let migrations = [
            Migration {
                version: 1,
                name: "good",
                apply: good,
            },
            Migration {
                version: 2,
                name: "bad",
                apply: bad,
            },
        ];

        let outcome = migrate_with(&connection, &migrations, &Actor::Human);

        match outcome {
            Err(StoreError::Io(message)) => {
                assert!(
                    message.contains("migration 2 (bad)"),
                    "names the migration: {message}"
                );
            }
            other => panic!("expected the failed migration to fail the open, got {other:?}"),
        }
        assert_eq!(
            version(&connection).unwrap(),
            Some(1),
            "only the good migration is recorded"
        );
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .unwrap();
        let tables: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert!(tables.iter().any(|name| name == "good"));
        assert!(
            !tables.iter().any(|name| name == "bad"),
            "the failed migration's table was rolled back"
        );
    }

    #[test]
    fn a_v1_file_upgrades_through_v2_and_keeps_its_rows() {
        let sandbox = Sandbox::new("upgrade");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..1], &Actor::Human).expect("v1 applies");
        connection
            .execute(
                "INSERT INTO tickets
                    (id, title, description, status, archived, lane, number, origin, assignee,
                     link, position, created_at, completed_at, author)
                 VALUES (?1, ?2, NULL, ?3, 0, NULL, NULL, ?4, NULL, NULL, 0, ?5, NULL, ?6)",
                rusqlite::params![
                    "00000000-0000-0000-0000-000000000001",
                    "kept",
                    "backlog",
                    "human",
                    "2026-10-30T00:00:00Z",
                    "human",
                ],
            )
            .expect("the v1 row lands");
        drop(connection);

        setting(&sandbox.store(), &Actor::Human, "k", "v");

        let connection = sandbox.connection();
        assert_eq!(version(&connection).unwrap(), Some(CURRENT_VERSION));
        assert!(
            has_index(&connection, "tickets_lane_position"),
            "v2 adds the board's index"
        );
        assert!(
            has_index(&connection, "comments_ticket"),
            "v2 adds the comment read's index"
        );
        assert!(
            super::super::schema::has_column(&connection, "sessions", "skills").unwrap(),
            "v3 adds the session record's skill counts"
        );
        let title: String = connection
            .query_row("SELECT title FROM tickets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(title, "kept", "the upgrade keeps every v1 row");

        let backup =
            Connection::open_with_flags(sandbox.backup(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .expect("the pre-migration image opens");
        assert_eq!(
            version(&backup).unwrap(),
            Some(1),
            "the migrating write rolls the pre-migration image"
        );
        assert!(
            !has_index(&backup, "tickets_lane_position"),
            "the pre-image predates v2"
        );
    }

    #[test]
    fn an_unreadable_database_fails_the_write() {
        let sandbox = Sandbox::new("not-a-database");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        fs::write(sandbox.database(), b"this is not a database").unwrap();

        let outcome = sandbox.store().write(&Actor::Human, |write| {
            write.insert_setting(NewSetting {
                key: "k",
                value: "v",
            })
        });

        match outcome {
            Err(StoreError::Io(message)) => assert!(!message.is_empty()),
            other => panic!("expected the open to fail loudly, got {other:?}"),
        }
    }

    #[test]
    fn a_write_takes_no_sidecar_lock() {
        use std::fs::OpenOptions;

        let sandbox = Sandbox::new("sidecar");
        let store = sandbox.store();
        setting(&store, &Actor::Human, "seed", "1");

        // ADR 0031 retires `tandem.lock` for the store: a write creates none,
        // and one another process holds is inert rather than a wait.
        let sidecar = sandbox.home_path().join("tandem.lock");
        assert!(
            !sidecar.exists(),
            "a store write created the retired sidecar at {}",
            sidecar.display()
        );

        let held = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&sidecar)
            .expect("the sidecar opens");
        held.try_lock().expect("the test takes the sidecar");

        let started = Instant::now();
        setting(&store, &Actor::Human, "next", "2");
        let elapsed = started.elapsed();
        drop(held);

        assert!(
            elapsed < Duration::from_millis(1_500),
            "the write waited {elapsed:?} on a sidecar the store must not take"
        );
    }

    /// The child half of [`another_process_contends_only_through_the_busy_window`].
    /// Spawned by the parent with the database in `TANDEM_TEST_HOLD_DB`; it holds
    /// the engine's write side until the parent closes its stdin.
    #[test]
    #[ignore = "spawned by another_process_contends_only_through_the_busy_window"]
    fn hold_the_write_side_for_another_process() {
        use std::io::Read;

        let Ok(database) = std::env::var("TANDEM_TEST_HOLD_DB") else {
            return;
        };
        let connection = Connection::open(&database).expect("the child opens the store");
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .expect("the child takes the write side");
        println!("held");
        let mut sink = Vec::new();
        std::io::stdin()
            .read_to_end(&mut sink)
            .expect("the child waits for the parent");
        connection
            .execute_batch("ROLLBACK")
            .expect("the child releases the write side");
    }

    #[test]
    fn another_process_contends_only_through_the_busy_window() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};

        let sandbox = Sandbox::new("other-process");
        let store = sandbox.store();
        setting(&store, &Actor::Human, "seed", "1");

        let mut child = Command::new(std::env::current_exe().expect("the test binary"))
            .args([
                "--exact",
                "store::tests::schema::hold_the_write_side_for_another_process",
                "--ignored",
                "--nocapture",
            ])
            .env("TANDEM_TEST_HOLD_DB", sandbox.database())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the child process starts");

        let stdout = child.stdout.take().expect("the child's stdout is piped");
        // The reader stays alive until the child exits: closing the pipe under
        // the harness's own output would turn its report into a broken pipe.
        let mut lines = BufReader::new(stdout).lines();
        let mut held = false;
        while let Some(Ok(line)) = lines.next() {
            if line.trim() == "held" {
                held = true;
                break;
            }
        }
        assert!(held, "the child never took the write side");

        let started = Instant::now();
        let outcome = store.write(&Actor::Human, |write| {
            write.insert_setting(NewSetting {
                key: "blocked",
                value: "1",
            })
        });
        let elapsed = started.elapsed();
        drop(child.stdin.take());
        let status = child.wait().expect("the child exits");
        drop(lines);
        assert!(status.success(), "the child failed: {status}");

        assert!(
            matches!(outcome, Err(StoreError::Locked { .. })),
            "another process's write must time out as locked: {outcome:?}"
        );
        assert!(
            elapsed >= Duration::from_millis(1_500),
            "the write waited the store's window (elapsed {elapsed:?})"
        );
        assert!(
            elapsed < Duration::from_secs(15),
            "the write did not wait far past the window (elapsed {elapsed:?})"
        );
        let count: i64 = sandbox
            .connection()
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1, "the timed-out write wrote nothing");
    }

    #[test]
    fn busy_timeout_surfaces_the_locked_error_after_the_window() {
        let sandbox = Sandbox::new("busy");
        let store = sandbox.store();
        setting(&store, &Actor::Human, "seed", "1");

        // Another connection holds the write side for longer than the store's
        // window: the engine's contention becomes the store's "locked".
        let holder = Connection::open(sandbox.database()).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();

        let started = Instant::now();
        let outcome = store.write(&Actor::Human, |write| {
            write.insert_setting(NewSetting {
                key: "blocked",
                value: "1",
            })
        });
        let elapsed = started.elapsed();
        holder.execute_batch("ROLLBACK").unwrap();

        assert!(
            matches!(outcome, Err(StoreError::Locked { .. })),
            "the write must time out as locked: {outcome:?}"
        );
        assert!(
            elapsed >= Duration::from_millis(1_500),
            "the write waited the store's window (elapsed {elapsed:?})"
        );
        assert!(
            elapsed < Duration::from_secs(15),
            "the write did not wait far past the window (elapsed {elapsed:?})"
        );
        let count: i64 = sandbox
            .connection()
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1, "the timed-out write wrote nothing");
    }
}

// MARK: - The v5 refusal

mod refusal {
    use super::*;

    /// A legacy v5 home in the shape the installed build leaves: the version
    /// seam at 5 and the ticket table it reads.
    fn v5_file(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let connection = Connection::open(path).expect("the legacy database opens");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, appliedAt TEXT NOT NULL);
                 INSERT INTO schema_migrations VALUES
                     (1, '2026-01-01T00:00:00Z'), (2, '2026-01-01T00:00:00Z'),
                     (3, '2026-01-01T00:00:00Z'), (4, '2026-01-01T00:00:00Z'),
                     (5, '2026-01-01T00:00:00Z');
                 CREATE TABLE tickets (id TEXT PRIMARY KEY, title TEXT NOT NULL);",
            )
            .expect("the legacy schema lands");
    }

    #[test]
    fn a_v5_file_is_refused_by_a_write_and_names_the_import() {
        let sandbox = Sandbox::new("refuse-write");
        v5_file(&sandbox.database());

        let outcome = sandbox.store().write(&Actor::Human, |write| {
            write.insert_setting(NewSetting {
                key: "k",
                value: "v",
            })
        });

        let error = match outcome {
            Err(error) => error,
            Ok(_) => panic!("a v5 file must be refused, not written"),
        };
        assert!(
            matches!(error, StoreError::Legacy { version: 5 }),
            "the refusal names the version: {error:?}"
        );
        assert!(
            error.to_string().contains("Scripts/import-legacy.py"),
            "the one error names the import: {error}"
        );
    }

    #[test]
    fn a_v5_file_is_refused_by_a_read_too() {
        let sandbox = Sandbox::new("refuse-read");
        v5_file(&sandbox.database());

        let outcome = sandbox.store().read(|connection| {
            connection
                .query_row("SELECT COUNT(*) FROM tickets", [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(|error| StoreError::Io(error.to_string()))
        });

        assert!(
            matches!(outcome, Err(StoreError::Legacy { version: 5 })),
            "a read refuses a v5 file as well: {outcome:?}"
        );
    }

    /// A clean legacy file: no journal beside it, so the version read is the
    /// only read and the refusal writes nothing. A v5 file carrying a hot
    /// journal is a different case: the read-write open rolls that journal back
    /// before the seam is read, which changes no row and no schema.
    #[test]
    fn a_refused_clean_file_is_left_as_it_was() {
        let sandbox = Sandbox::new("refuse-untouched");
        v5_file(&sandbox.database());
        let before = fs::read(sandbox.database()).unwrap();

        let _ = sandbox.store().write(&Actor::Human, |write| {
            write.insert_setting(NewSetting {
                key: "k",
                value: "v",
            })
        });

        let connection = Connection::open(sandbox.database()).unwrap();
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        let tables: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(
            tables,
            vec!["schema_migrations".to_string(), "tickets".to_string()],
            "the refusal creates no table"
        );
        assert_eq!(
            fs::read(sandbox.database()).unwrap(),
            before,
            "the refusal writes no byte"
        );
        assert!(
            !sandbox.home_path().join("tandem.db-wal").exists(),
            "the refusal does not switch the file's journal mode"
        );
    }
}

// MARK: - The actor on every write

mod actor {
    use super::*;

    fn a_full_write(store: &Store, actor: &Actor) {
        store
            .write(actor, |write| {
                write.insert_lane(NewLane {
                    name: "Port",
                    key: "PRT",
                    last_number: 4,
                    position: 0,
                })?;
                write.insert_ticket(NewTicket {
                    id: "00000000-0000-0000-0000-000000000004",
                    title: "the store adapter",
                    description: Some("the body"),
                    status: "backlog",
                    archived: false,
                    lane: Some("Port"),
                    number: Some(4),
                    origin: "human",
                    assignee: None,
                    link: None,
                    position: 0,
                    created_at: "2026-10-30T00:00:00Z",
                    completed_at: None,
                })?;
                write.insert_dependency(NewDependency {
                    ticket_id: "00000000-0000-0000-0000-000000000004",
                    depends_on_id: "00000000-0000-0000-0000-000000000003",
                })?;
                write.insert_comment(NewComment {
                    ticket_id: "00000000-0000-0000-0000-000000000004",
                    at: "2026-10-30T00:00:01Z",
                    body: "a comment",
                })?;
                write.insert_session(NewSession {
                    id: "ses_e73388dd8ffd",
                    harness_session_id: Some("ses_harness"),
                    harness_transport: Some("database"),
                    project: "tandem",
                    agent: Some("builder"),
                    brief: "Build the store.",
                    status: "running",
                    outcome: None,
                    summary: None,
                    started_at: "2026-10-30T00:00:00Z",
                    ended_at: None,
                    tickets: Some("[\"5E345FA3\"]"),
                    model: Some("deepseek-v4.1-flash"),
                    tokens_input: Some(10),
                    tokens_output: Some(20),
                    tokens_reasoning: Some(0),
                    tokens_cache_read: Some(0),
                    tokens_cache_write: Some(0),
                    seconds: Some(1.5),
                    cost: Some(0.01),
                    skills: Some("{\"thinking\":2}"),
                })?;
                write.insert_sprint(NewSprint {
                    lane: "Port",
                    number: 1,
                    session: Some("ses_e73388dd8ffd"),
                    mode: "serial",
                    batch: Some("[]"),
                    runs: Some("[]"),
                    close: None,
                    status: "open",
                    opened_at: "2026-10-30T00:00:00Z",
                    closed_at: None,
                })?;
                write.insert_setting(NewSetting {
                    key: "defaultLane",
                    value: "Port",
                })?;
                Ok(())
            })
            .expect("the full write lands");
    }

    #[test]
    fn an_unset_environment_is_the_human() {
        assert_eq!(Actor::parse(None), Actor::Human);
        assert_eq!(Actor::parse(Some("")), Actor::Human);
        assert_eq!(Actor::parse(Some("   ")), Actor::Human);
        assert_eq!(Actor::Human.as_str(), "human");
    }

    #[test]
    fn a_pass_name_is_the_agent() {
        assert_eq!(
            Actor::parse(Some(" builder ")),
            Actor::Agent("builder".to_string())
        );
        assert_eq!(Actor::Agent("builder".to_string()).as_str(), "builder");
        assert_eq!(ACTOR_ENVIRONMENT, "TANDEM_AGENT");
    }

    #[test]
    fn every_table_records_its_author() {
        let sandbox = Sandbox::new("authors");
        let store = sandbox.store();
        let builder = Actor::Agent("builder".to_string());

        a_full_write(&store, &builder);

        let connection = sandbox.connection();
        for table in AUTHORED_TABLES {
            let authors = author_of(&connection, table);
            assert!(!authors.is_empty(), "{table} carries a row");
            assert!(
                authors.iter().all(|author| author == "builder"),
                "every row of {table} carries the write's actor, got {authors:?}"
            );
        }

        // The next write records its own actor, not the last one's.
        setting(&store, &Actor::Human, "other", "1");
        let settings = author_of(&connection, "settings");
        assert_eq!(settings, vec!["builder".to_string(), "human".to_string()]);
        assert_eq!(
            author_of(&connection, "tickets"),
            vec!["builder".to_string()],
            "a later write does not rewrite a row it did not touch"
        );
    }

    #[test]
    fn a_row_does_not_infer_its_author_from_a_session() {
        let sandbox = Sandbox::new("no-inference");
        let store = sandbox.store();

        // A session row names the pass in `agent`; a ticket written by the human
        // must still read `human`, because the author is the column, never the
        // session it might be linked to.
        a_full_write(&store, &Actor::Human);

        let connection = sandbox.connection();
        let agent: Option<String> = connection
            .query_row("SELECT agent FROM sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            agent.as_deref(),
            Some("builder"),
            "the fixture names a pass in the session row"
        );
        assert_eq!(
            author_of(&connection, "tickets"),
            vec!["human".to_string()],
            "the ticket's author is the write's actor"
        );
    }
}

// MARK: - The rolling backup and concurrency

mod writes {
    use std::thread;

    use super::*;

    #[test]
    fn the_first_write_has_no_backup() {
        let sandbox = Sandbox::new("backup-first");
        setting(&sandbox.store(), &Actor::Human, "one", "1");

        assert!(
            !sandbox.backup().exists(),
            "the first write has no file to roll"
        );
    }

    #[test]
    fn the_second_writes_backup_holds_the_state_before_it() {
        let sandbox = Sandbox::new("backup-second");
        let store = sandbox.store();
        setting(&store, &Actor::Human, "one", "1");
        setting(&store, &Actor::Human, "two", "2");

        let backup =
            Connection::open_with_flags(sandbox.backup(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .expect("the backup opens");
        let mut statement = backup
            .prepare("SELECT key FROM settings ORDER BY rowid")
            .unwrap();
        let keys: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(
            keys,
            vec!["one".to_string()],
            "the backup is the state before the second write"
        );
    }

    #[test]
    fn a_no_op_write_rolls_no_backup() {
        let sandbox = Sandbox::new("backup-noop");
        let store = sandbox.store();
        setting(&store, &Actor::Human, "one", "1");

        store
            .write(&Actor::Human, |_write| Ok(()))
            .expect("a no-op write lands");

        assert!(
            !sandbox.backup().exists(),
            "a write that changed nothing rolls no backup"
        );
    }

    #[test]
    fn a_read_never_rolls_a_backup() {
        let sandbox = Sandbox::new("backup-read");
        let store = sandbox.store();
        setting(&store, &Actor::Human, "one", "1");
        setting(&store, &Actor::Human, "two", "2");
        let before = fs::metadata(sandbox.backup()).unwrap().modified().unwrap();

        for _ in 0..5 {
            store.read(|_| Ok(())).expect("the read answers");
            store.schema_version().expect("the version reads");
        }

        let after = fs::metadata(sandbox.backup()).unwrap().modified().unwrap();
        assert_eq!(after, before, "a read rolls no backup");
    }

    #[test]
    fn the_next_write_sweeps_a_backup_temp_a_kill_left_behind() {
        let sandbox = Sandbox::new("backup-orphan");
        let store = sandbox.store();
        setting(&store, &Actor::Human, "one", "1");

        // A writer killed mid-backup leaves its same-directory temp: nothing
        // renames it and no read removes it, so the next write sweeps it.
        let killed = sandbox.home_path().join(".tandem-backup-4242-1-0.tmp");
        fs::write(&killed, b"half a copy").unwrap();
        aged(&killed, 120);
        // A temp young enough to belong to a writer still running is not a
        // kill's, and the sweep must leave it alone.
        let in_flight = sandbox.home_path().join(".tandem-backup-4243-2-0.tmp");
        fs::write(&in_flight, b"a copy in progress").unwrap();

        setting(&store, &Actor::Human, "two", "2");

        assert!(
            !killed.exists(),
            "the next write left the killed backup's temp at {}",
            killed.display()
        );
        assert!(
            in_flight.exists(),
            "the sweep removed a temp a live writer may still hold"
        );
        let _ = fs::remove_file(&in_flight);
    }

    #[test]
    fn a_backup_that_cannot_be_landed_fails_the_write_and_changes_nothing() {
        let sandbox = Sandbox::new("backup-fails");
        let store = sandbox.store();
        setting(&store, &Actor::Human, "one", "1");
        setting(&store, &Actor::Human, "two", "2");

        // The backup's destination is a directory, so the atomic rename cannot
        // land it: the write must fail whole and change nothing.
        fs::remove_file(sandbox.backup()).unwrap();
        fs::create_dir(sandbox.backup()).unwrap();

        let outcome = store.write(&Actor::Human, |write| {
            write.insert_setting(NewSetting {
                key: "three",
                value: "3",
            })
        });

        match outcome {
            Err(StoreError::Io(message)) => assert!(
                message.contains("backup"),
                "the failure names the backup: {message}"
            ),
            other => panic!("expected the backup to fail the write, got {other:?}"),
        }
        let count: i64 = sandbox
            .connection()
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2, "the failed write changed nothing");

        fs::remove_dir(sandbox.backup()).unwrap();
        setting(&store, &Actor::Human, "three", "3");
        let count: i64 = sandbox
            .connection()
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3, "the store answers again once the backup can land");
    }

    #[test]
    fn the_environment_home_is_honored_and_the_user_home_is_never_touched() {
        let sandbox = Sandbox::new("environment");
        let environment = sandbox.root().join("environment-home");
        let home = Home::resolve(
            None,
            Some(environment.to_str().unwrap()),
            &sandbox.user_home(),
        );

        Store::new(home)
            .write(&Actor::Human, |write| {
                write.insert_setting(NewSetting {
                    key: "k",
                    value: "v",
                })
            })
            .expect("the write lands under the environment home");

        assert!(
            environment.join("tandem.db").exists(),
            "TANDEM_HOME is the home the write used"
        );
        assert!(
            !sandbox.user_home().join(".tandem").exists(),
            "~/.tandem is never created when TANDEM_HOME is set"
        );
        assert!(
            !sandbox.home_path().exists(),
            "the flag's home is not a fallback once the environment names one"
        );
    }

    #[test]
    fn concurrent_writers_keep_every_row() {
        let sandbox = Sandbox::new("concurrent");
        let store = sandbox.store();
        setting(&store, &Actor::Human, "seed", "1");

        // The timing assumption: the only thing these two threads contend for
        // is the process-local write gate (`store::write_gate`), one mutex per
        // canonical home, so `BEGIN IMMEDIATE` never sees a same-process
        // sibling: a writer waits one write of the other, never the 2 s busy
        // window. Across processes the window is the bound (ADR 0031). No
        // ordering between the threads is assumed: the count is read after the
        // scope joins both.
        thread::scope(|scope| {
            for group in ["a", "b"] {
                let store = &store;
                scope.spawn(move || {
                    for index in 0..10 {
                        let key = format!("{group}-{index}");
                        let started = Instant::now();
                        store
                            .write(&Actor::Agent(group.to_string()), |write| {
                                write.insert_setting(NewSetting {
                                    key: &key,
                                    value: "1",
                                })
                            })
                            .unwrap_or_else(|error| {
                                panic!(
                                    "{key} answered {error} after {:?}; the store takes no \
                                     sidecar lock, so the busy window is the only wait",
                                    started.elapsed()
                                )
                            });
                    }
                });
            }
        });

        let count: i64 = sandbox
            .connection()
            .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 21, "no concurrent writer lost a row");
    }

    #[test]
    fn a_lock_free_reader_sees_a_consistent_snapshot_while_a_writer_works() {
        let sandbox = Sandbox::new("snapshot");
        let store = sandbox.store();
        setting(&store, &Actor::Human, "seed", "1");

        let mut last_seen = 1;
        let mut reads = 0;
        thread::scope(|scope| {
            let writer = scope.spawn(|| {
                for index in 0..20 {
                    let key = format!("w-{index}");
                    store
                        .write(&Actor::Human, |write| {
                            write.insert_setting(NewSetting {
                                key: &key,
                                value: "1",
                            })
                        })
                        .expect("the writer's row lands");
                }
            });

            // The invariant under test is here, inside the loop: every read is a
            // complete committed state, monotone and never ahead of the writer.
            // It is not that this loop saw the final write: it exits when the
            // writer's thread ends, and its last read may precede the writer's
            // last commit.
            while !writer.is_finished() {
                let count = store
                    .read(|connection| {
                        connection
                            .query_row("SELECT COUNT(*) FROM settings", [], |row| {
                                row.get::<_, i64>(0)
                            })
                            .map_err(|error| StoreError::Io(error.to_string()))
                    })
                    .expect("the read answers")
                    .unwrap_or(0);
                assert!(
                    count >= last_seen,
                    "a read lost committed rows: {last_seen} -> {count}"
                );
                assert!(
                    count <= 21,
                    "a read saw rows the writer had not committed: {count}"
                );
                last_seen = count;
                reads += 1;
            }
            writer.join().expect("the writer finishes");
        });

        assert!(reads > 0, "the reader never ran");
        let total = store
            .read(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM settings", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map_err(|error| StoreError::Io(error.to_string()))
            })
            .expect("the read after the join answers")
            .unwrap_or(0);
        assert_eq!(
            total, 21,
            "every row the writer committed is in the store once it finished"
        );
    }
}

// MARK: - The seam's floor

#[test]
fn bundled_sqlite_answers() {
    let connection = rusqlite::Connection::open_in_memory().expect("an in-memory engine opens");
    let version: String = connection
        .query_row("select sqlite_version()", [], |row| row.get(0))
        .expect("the engine answers");
    assert!(!version.is_empty(), "SQLite reports its version");
}

// The restore below renames the backup over the store while a watch holds the
// file open. Windows refuses to remove or rename a file another handle has
// open (SQLite's VFS does not ask for `FILE_SHARE_DELETE`), so this case is
// unix only and the documented restore is the app-closed one there. The half
// the platform can run, a rollback-journal store answering the watch with no
// rename over a held file, is
// `a_rollback_journal_store_answers_a_revision_to_the_watch` below, and the
// pipe suite in `cli/tests/rpc.rs` covers the same shape through the child
// (PRT-26).
#[cfg(unix)]
#[test]
fn a_restored_store_is_watched_without_reopening() {
    let sandbox = Sandbox::new("watch-restore");
    let store = sandbox.store();
    setting(&store, &Actor::Human, "one", "1");
    setting(&store, &Actor::Human, "two", "2");

    let watch = store
        .watch()
        .expect("the store opens")
        .expect("a WAL store is watchable");
    let before = watch.revision().expect("the revision reads");

    // ADR 0031's restore: remove the journal files and the database, then move
    // the rollback-journal backup over the path.
    let home = sandbox.home_path();
    let _ = fs::remove_file(home.join("tandem.db-wal"));
    let _ = fs::remove_file(home.join("tandem.db-shm"));
    fs::remove_file(sandbox.database()).expect("the database is removable");
    fs::rename(sandbox.backup(), sandbox.database()).expect("the backup moves over the path");

    assert!(
        watch.revision().is_err(),
        "the held watch notices the replacement"
    );

    // A restored store is watchable: store_meta reads in rollback-journal mode,
    // so the stream re-attaches instead of going blind.
    let restored = store
        .watch()
        .expect("the restored store opens")
        .expect("a restored store is watchable");
    let revision = restored.revision().expect("store_meta reads without WAL");
    assert!(revision < before, "the backup is an earlier revision");

    let opens = store.watch_opens();
    for _ in 0..10 {
        restored.revision().expect("the watch reads");
    }
    assert_eq!(
        store.watch_opens(),
        opens,
        "a held watch does not open a connection per read"
    );

    // The held read-only connection does not block the next write's one-time
    // switch back to WAL.
    setting(&store, &Actor::Human, "three", "3");
    assert!(restored.revision().expect("the watch reads") > revision);
}

// The restore's other half, and the only half Windows can run: the app is
// closed for the restore (the documented recipe says so on Windows, PRT-26),
// so nothing is renamed over a held file. The store the restore leaves is the
// rollback-journal backup copied into place, and the watch has to attach to it
// and answer its revision. Copying the backup into a home of its own produces
// exactly that file without the rename the platform forbids.
#[test]
fn a_rollback_journal_store_answers_a_revision_to_the_watch() {
    let sandbox = Sandbox::new("watch-rollback-journal");
    let store = sandbox.store();
    setting(&store, &Actor::Human, "one", "1");
    setting(&store, &Actor::Human, "two", "2");
    let before = store.revision().expect("the store reads");

    let restored_home = sandbox.root().join("restored");
    fs::create_dir_all(&restored_home).expect("the restored home is creatable");
    let restored_database = restored_home.join("tandem.db");
    fs::copy(sandbox.backup(), &restored_database).expect("the backup copies");
    let restored = Store::new(Home::resolve(
        Some(&restored_home),
        None,
        &sandbox.user_home(),
    ));

    // The file is what ADR 0031's restore leaves: the backup, in
    // rollback-journal mode, so it opens as one file with no `-shm` beside it.
    let connection =
        Connection::open_with_flags(&restored_database, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("the restored database opens");
    assert_eq!(
        journal_mode(&connection)
            .expect("the journal mode reads")
            .as_deref(),
        Some("delete"),
        "the backup is a rollback-journal file"
    );
    drop(connection);

    // A rollback-journal store is watchable, and the watch reports the
    // revision the backup was taken at: the stream is not blind until the next
    // write switches WAL back on.
    let watch = restored
        .watch()
        .expect("the restored store opens")
        .expect("a rollback-journal store is watchable");
    let revision = watch.revision().expect("store_meta reads without WAL");
    assert!(revision > 0, "the backup carries its revision");
    assert!(revision < before, "the backup predates the second write");

    let opens = restored.watch_opens();
    for _ in 0..10 {
        watch.revision().expect("the watch reads");
    }
    assert_eq!(
        restored.watch_opens(),
        opens,
        "a held watch does not open a connection per read"
    );

    // The next write switches WAL back on, and the held connection reads it:
    // a child that re-attached keeps reporting instead of going silent.
    setting(&restored, &Actor::Human, "three", "3");
    assert!(watch.revision().expect("the watch reads") > revision);
}
