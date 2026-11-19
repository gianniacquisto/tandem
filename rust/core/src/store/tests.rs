//! The store's suite: the write discipline (ADR 0001, ADR 0007), the
//! SQLite seam (ADR 0031), the rolling backup (ADR 0018), the actor on every
//! write, and the v5 refusal.
//!
//! The store's seam suite: the behaviours it asserts over the storage. This one is
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

/// A write that leaves one live row: a project named `name`. The write cases
/// use it where they only need a transaction that changes something.
fn probe(store: &Store, actor: &Actor, name: &str) {
    store
        .write(actor, |write| {
            write.insert_project(NewProject { name, position: 0 })
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
const AUTHORED_TABLES: [&str; 6] = [
    "tickets",
    "ticket_dependencies",
    "comments",
    "labels",
    "projects",
    "schema_migrations",
];

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

        let held = enter_gate(&gate, &home, BUSY_TIMEOUT).expect("the gate is taken");
        let (tx, entered) = mpsc::channel();
        let queued = {
            let gate = Arc::clone(&gate);
            let home = home.clone();
            thread::spawn(move || {
                let _guard = enter_gate(&gate, &home, BUSY_TIMEOUT)
                    .expect("the queued writer takes the gate");
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
        let held = enter_gate(&gate, &home, BUSY_TIMEOUT).expect("the test takes the gate");
        gate.wait_passes.store(0, Ordering::Relaxed);

        let (tx, answered) = mpsc::channel();
        let waiting = {
            let gate = Arc::clone(&gate);
            let home = home.clone();
            thread::spawn(move || {
                let started = Instant::now();
                let outcome = enter_gate(&gate, &home, BUSY_TIMEOUT).map(|_guard| ());
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

    /// The window a test store configures is the window that bites: a
    /// deliberate hold longer than the configured window still answers
    /// `Locked`, so the contention case's wider window is a setting, not a
    /// disabled bound.
    #[test]
    fn a_configured_gate_window_still_bites_after_a_deliberate_hold() {
        let sandbox = Sandbox::new("gate-configured-deadline");
        let home = sandbox.home_path();
        fs::create_dir_all(&home).unwrap();
        let gate = write_gate(&home);
        let held = enter_gate(&gate, &home, BUSY_TIMEOUT).expect("the test takes the gate");
        let store = sandbox
            .store()
            .with_gate_timeout(Duration::from_millis(200));

        let (tx, answered) = mpsc::channel();
        let queued = thread::spawn(move || {
            let started = Instant::now();
            let outcome = store.write(&Actor::Human, |write| {
                write.insert_project(NewProject {
                    name: "blocked",
                    position: 0,
                })
            });
            tx.send((outcome, started.elapsed()))
                .expect("the parent is listening");
        });

        let (outcome, elapsed) = answered
            .recv_timeout(Duration::from_secs(5))
            .expect("a held gate must answer within the configured window");
        drop(held);
        queued.join().expect("the queued writer finishes");

        assert!(
            matches!(&outcome, Err(StoreError::Locked { home: named }) if named == &home),
            "a held gate must time out as locked, naming the home: {outcome:?}"
        );
        assert!(
            elapsed >= Duration::from_millis(150),
            "the writer waited the configured window (elapsed {elapsed:?})"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "the writer did not wait far past the configured window (elapsed {elapsed:?})"
        );
    }

    #[test]
    fn a_held_gate_times_out_as_locked() {
        let sandbox = Sandbox::new("gate-deadline");
        let home = sandbox.home_path();
        fs::create_dir_all(&home).unwrap();
        let gate = write_gate(&home);
        let held = enter_gate(&gate, &home, BUSY_TIMEOUT).expect("the test takes the gate");

        // The engine is free while this thread holds the gate, so only the
        // gate's own deadline can turn the queued writer away.
        let store = sandbox.store();
        let (tx, answered) = mpsc::channel();
        let queued = thread::spawn(move || {
            let started = Instant::now();
            let outcome = store.write(&Actor::Human, |write| {
                write.insert_project(NewProject {
                    name: "blocked",
                    position: 0,
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
    use super::super::schema::{
        CURRENT_VERSION, DEFAULT_WORKFLOW_ID, MIGRATIONS, Migration, has_column, has_table,
        migrate_with, version,
    };
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

        probe(&store, &Actor::Human, "defaultLabel");

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
            super::super::decode(&applied_at).is_some(),
            "applied_at is an ISO-8601 UTC timestamp: {applied_at}"
        );
    }

    #[test]
    fn the_tables_are_the_ones_the_format_names() {
        let sandbox = Sandbox::new("tables");
        probe(&sandbox.store(), &Actor::Human, "Tandem");

        let connection = sandbox.connection();
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
            vec![
                "access_record",
                "browser_sessions",
                "comments",
                "labels",
                "member_signin_keys",
                "member_tokens",
                "members",
                "projects",
                "schema_migrations",
                "store_meta",
                "ticket_dependencies",
                "tickets",
            ],
            "a fresh home carries only the live tables"
        );
    }

    /// A v10 home still carries the five tables the cut retired; its next write
    /// drops them and keeps every live row.
    #[test]
    fn the_dead_tables_are_dropped_on_a_migrated_home() {
        let sandbox = Sandbox::new("drop-dead");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..10], &Actor::Human).expect("v10 applies");
        connection
            .execute_batch(
                "INSERT INTO sessions (id, project, brief, status, started_at, author)
                   VALUES ('s1', 'tandem', 'a pass', 'running', '2026-11-01T00:00:00Z', 'human');
                 INSERT INTO settings (key, value, author) VALUES ('k', 'v', 'human');
                 INSERT INTO workflow_runs (lane, number, mode, status, opened_at, author)
                   VALUES ('cut', 1, 'serial', 'closed', '2026-11-01T00:00:00Z', 'human');",
            )
            .expect("the v10 rows land");
        drop(connection);

        probe(&sandbox.store(), &Actor::Human, "kept");

        let connection = sandbox.connection();
        for table in [
            "sessions",
            "sprints",
            "settings",
            "workflows",
            "workflow_stages",
            "workflow_runs",
        ] {
            assert!(
                !has_table(&connection, table).unwrap(),
                "{table} must be dropped by the migration"
            );
        }
        let kept: i64 = connection
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(kept, 1, "the live row the write made survives the drop");
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
                super::super::decode(&applied_at.unwrap()).is_some(),
                "every applied migration carries an ISO-8601 UTC timestamp"
            );
        }
    }

    #[test]
    fn a_second_migrate_applies_nothing() {
        let sandbox = Sandbox::new("migrate-twice");
        let store = sandbox.store();
        probe(&store, &Actor::Human, "k");

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

        probe(&sandbox.store(), &Actor::Human, "kept");

        let connection = sandbox.connection();
        assert_eq!(version(&connection).unwrap(), Some(CURRENT_VERSION));
        assert!(
            has_index(&connection, "tickets_label_position"),
            "v2 adds the board's index"
        );
        assert!(
            has_index(&connection, "comments_ticket"),
            "v2 adds the comment read's index"
        );
        assert!(
            !super::super::schema::has_table(&connection, "sessions").unwrap(),
            "v11 drops the session table"
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
            !has_index(&backup, "tickets_label_position"),
            "the pre-image predates v2"
        );
    }

    #[test]
    fn a_v5_file_upgrades_to_v9_with_projects_and_the_agile_workflow() {
        let sandbox = Sandbox::new("upgrade-v6");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..5], &Actor::Human).expect("v5 applies");
        for (name, key, last_number, position) in [("Tandem", "TAN", 2, 0), ("Port", "PRT", 1, 1)] {
            connection
                .execute(
                    "INSERT INTO lanes (name, key, last_number, position, author)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![name, key, last_number, position, "human"],
                )
                .expect("the v5 lane lands");
        }
        for (id, title, lane, number, position) in [
            (
                "00000000-0000-0000-0000-000000000001",
                "first",
                Some("Tandem"),
                Some(1),
                0,
            ),
            (
                "00000000-0000-0000-0000-000000000002",
                "second",
                Some("Tandem"),
                Some(2),
                1,
            ),
            (
                "00000000-0000-0000-0000-000000000003",
                "ported",
                Some("Port"),
                Some(1),
                2,
            ),
            (
                "00000000-0000-0000-0000-000000000004",
                "capture",
                None,
                None,
                3,
            ),
        ] {
            connection
                .execute(
                    "INSERT INTO tickets
                        (id, title, description, status, archived, lane, number, origin,
                         assignee, link, position, created_at, completed_at, author)
                     VALUES (?1, ?2, NULL, 'backlog', 0, ?3, ?4, 'human', NULL, NULL, ?5,
                             '2026-10-30T00:00:00Z', NULL, 'human')",
                    rusqlite::params![id, title, lane, number, position],
                )
                .expect("the v5 ticket lands");
        }
        drop(connection);

        // Stop at v9: v11 drops the workflow tables this case reads.
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..9], &Actor::Human).expect("v9 applies");

        assert_eq!(
            version(&connection).unwrap(),
            Some(9),
            "the workflow's last version"
        );
        let mut projects: Vec<(String, i64)> = connection
            .prepare("SELECT name, position FROM projects ORDER BY position, rowid")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        projects.sort_by_key(|left| left.1);
        assert_eq!(
            projects,
            vec![("Tandem".to_string(), 0), ("Port".to_string(), 1),],
            "one project per lane, order follows lane order"
        );
        let lanes: Vec<(String, String, i64, i64)> = connection
            .prepare("SELECT name, key, last_number, position FROM lanes ORDER BY position, rowid")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(
            lanes,
            vec![
                ("Tandem".to_string(), "TAN".to_string(), 2, 0),
                ("Port".to_string(), "PRT".to_string(), 1, 1),
            ],
            "lane names, keys, numbers and positions are unchanged"
        );
        let parents: Vec<(String, String)> = connection
            .prepare("SELECT name, project FROM lanes ORDER BY position, rowid")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(
            parents,
            vec![
                ("Tandem".to_string(), "Tandem".to_string()),
                ("Port".to_string(), "Port".to_string()),
            ],
            "each lane's parent is the project named for it"
        );
        let tickets: Vec<(String, Option<String>, Option<i64>, i64)> = connection
            .prepare("SELECT id, lane, number, position FROM tickets ORDER BY position, rowid")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(
            tickets,
            vec![
                (
                    "00000000-0000-0000-0000-000000000001".to_string(),
                    Some("Tandem".to_string()),
                    Some(1),
                    0,
                ),
                (
                    "00000000-0000-0000-0000-000000000002".to_string(),
                    Some("Tandem".to_string()),
                    Some(2),
                    1,
                ),
                (
                    "00000000-0000-0000-0000-000000000003".to_string(),
                    Some("Port".to_string()),
                    Some(1),
                    2,
                ),
                (
                    "00000000-0000-0000-0000-000000000004".to_string(),
                    None,
                    None,
                    3,
                ),
            ],
            "refs, numbers, positions and the default-area ticket are untouched"
        );
        let workflows: Vec<(String, String)> = connection
            .prepare("SELECT id, name FROM workflows")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(
            workflows,
            vec![(
                "00000000-0000-4000-8000-000000000001".to_string(),
                "agile".to_string()
            )],
            "the v7 migration seeds the one default workflow"
        );
        let stages: Vec<(String, String, String)> = connection
            .prepare(
                "SELECT key, label, role FROM workflow_stages
                 WHERE workflow_id = '00000000-0000-4000-8000-000000000001'
                 ORDER BY position",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(
            stages,
            vec![
                (
                    "backlog".to_string(),
                    "Backlog".to_string(),
                    "intake".to_string()
                ),
                (
                    "on-hold".to_string(),
                    "On hold".to_string(),
                    "parked".to_string()
                ),
                (
                    "selected".to_string(),
                    "Selected for development".to_string(),
                    "intake".to_string()
                ),
                (
                    "ready".to_string(),
                    "Ready".to_string(),
                    "ready".to_string()
                ),
                (
                    "in-progress".to_string(),
                    "In progress".to_string(),
                    "active".to_string()
                ),
                (
                    "in-review".to_string(),
                    "In Review".to_string(),
                    "review".to_string()
                ),
                (
                    "done".to_string(),
                    "Done".to_string(),
                    "finished".to_string()
                ),
            ],
            "the default carries every existing key plus the selected stage"
        );
        let workflow: Option<String> = connection
            .query_row("SELECT workflow FROM projects LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(workflow, None, "projects.workflow starts unset");
    }

    #[test]
    fn a_v6_file_with_rows_upgrades_to_v7_keeping_tickets_sprints_and_the_default_stage_set() {
        let sandbox = Sandbox::new("upgrade-v7");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..6], &Actor::Human).expect("v6 applies");
        connection
            .execute(
                "INSERT INTO tickets
                    (id, title, status, archived, lane, number, origin, assignee, position,
                     created_at, completed_at, author)
                 VALUES ('00000000-0000-0000-0000-0000000000aa', 'kept', 'in-review', 0,
                         NULL, NULL, 'human', NULL, 0, '2026-10-30T00:00:00Z', NULL, 'human')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO sprints
                    (lane, number, session, mode, batch, runs, close, status, opened_at,
                     closed_at, author)
                 VALUES ('Tandem', 1, NULL, 'serial', '[]', '[]', NULL, 'closed',
                         '2026-10-30T00:00:00Z', '2026-10-30T00:00:00Z', 'human')",
                [],
            )
            .unwrap();
        drop(connection);

        // Stop at v7, the version this case names: v11 drops the workflow and
        // run tables it reads.
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..7], &Actor::Human).expect("v7 applies");
        let connection = sandbox.connection();
        assert_eq!(version(&connection).unwrap(), Some(7));
        let status: String = connection
            .query_row("SELECT status FROM tickets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(status, "in-review", "an existing ticket keeps its status");
        let stage: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM workflow_stages WHERE key = 'in-review'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stage, 1, "every existing status is a default stage");
        let run: String = connection
            .query_row("SELECT status FROM sprints", [], |row| row.get(0))
            .unwrap();
        assert_eq!(run, "closed", "the run row is untouched");
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn a_v8_file_upgrades_to_v10_renaming_sprints_to_workflow_runs_with_the_default_workflow() {
        let sandbox = Sandbox::new("upgrade-v10");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..8], &Actor::Human).expect("v8 applies");
        connection
            .execute(
                "INSERT INTO sprints
                    (lane, number, session, mode, batch, runs, close, status, opened_at,
                     closed_at, author)
                 VALUES ('Tandem', 1, 'ses_closed', 'parallel', '[\"a\"]', '[\"note\"]',
                         'Landed.', 'closed', '2026-10-30T00:00:00Z',
                         '2026-10-31T00:00:00Z', 'human')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO sprints
                    (lane, number, session, mode, batch, runs, close, status, opened_at,
                     closed_at, author)
                 VALUES ('Tandem', 2, 'ses_open', 'serial', '[]', '[]', NULL, 'open',
                         '2026-11-01T00:00:00Z', NULL, 'human')",
                [],
            )
            .unwrap();
        drop(connection);

        // Stop at v10, the version this case names: v11 drops the run table.
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..10], &Actor::Human).expect("v10 applies");
        let connection = sandbox.connection();
        assert_eq!(version(&connection).unwrap(), Some(10));
        assert!(
            !has_table(&connection, "sprints").unwrap(),
            "the v8 table is renamed in place, not copied"
        );
        let rows: Vec<(
            String,
            i64,
            String,
            Option<String>,
            String,
            String,
            String,
            String,
            String,
        )> = connection
            .prepare(
                "SELECT lane, number, workflow, session, mode, batch, runs, status, author
                     FROM workflow_runs ORDER BY number",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            })
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(
            rows,
            vec![
                (
                    "Tandem".to_string(),
                    1,
                    DEFAULT_WORKFLOW_ID.to_string(),
                    Some("ses_closed".to_string()),
                    "parallel".to_string(),
                    "[\"a\"]".to_string(),
                    "[\"note\"]".to_string(),
                    "closed".to_string(),
                    "human".to_string(),
                ),
                (
                    "Tandem".to_string(),
                    2,
                    DEFAULT_WORKFLOW_ID.to_string(),
                    Some("ses_open".to_string()),
                    "serial".to_string(),
                    "[]".to_string(),
                    "[]".to_string(),
                    "open".to_string(),
                    "human".to_string(),
                ),
            ],
            "both rows keep their key and every field, and gain the default workflow"
        );
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn the_v10_migration_closes_empty_null_session_orphan_runs_and_keeps_active_open_runs() {
        let sandbox = Sandbox::new("upgrade-v10-orphans");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..8], &Actor::Human).expect("v8 applies");
        for (lane, number, session, batch, opened_at) in [
            ("Hosting", 1, None::<&str>, "[]", "2026-11-04T00:17:44Z"),
            ("Retro", 2, None, "[]", "2026-11-04T00:17:02Z"),
            ("Tandem", 1, Some("ses_live"), "[]", "2026-11-04T00:18:00Z"),
            ("Tandem", 2, None, "[\"TAN-1\"]", "2026-11-04T00:19:00Z"),
        ] {
            connection
                .execute(
                    "INSERT INTO sprints
                        (lane, number, session, mode, batch, runs, close, status, opened_at,
                         closed_at, author)
                     VALUES (?1, ?2, ?3, 'serial', ?4, '[]', NULL, 'open', ?5, NULL, 'human')",
                    rusqlite::params![lane, number, session, batch, opened_at],
                )
                .unwrap();
        }
        drop(connection);

        // Stop at v10: v11 drops the run table this case reads.
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..10], &Actor::Human).expect("v10 applies");
        let connection = sandbox.connection();
        assert_eq!(version(&connection).unwrap(), Some(10));
        let rows: Vec<(String, i64, String, Option<String>, Option<String>)> = connection
            .prepare("SELECT lane, number, status, closed_at, close FROM workflow_runs ORDER BY lane, number")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(
            rows[0],
            (
                "Hosting".to_string(),
                1,
                "closed".to_string(),
                Some("2026-11-04T00:17:44Z".to_string()),
                Some(
                    "Closed by schema v10 migration: empty run opened with no session.".to_string()
                ),
            ),
            "an empty null-session open run closes with its provenance"
        );
        assert_eq!(rows[1].2, "closed", "the second orphan closes too");
        assert_eq!(
            rows[2],
            ("Tandem".to_string(), 1, "open".to_string(), None, None,),
            "an open run carrying a session stays open"
        );
        assert_eq!(rows[3].2, "open", "an open run carrying a batch stays open");
    }

    #[test]
    fn a_v11_file_migrates_to_v12_renaming_lanes_to_labels_and_changing_nothing_else() {
        use crate::domain::{Board, TicketQuery};

        let sandbox = Sandbox::new("v11-to-v12");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..11], &Actor::Human).expect("v11 applies");
        connection
            .execute_batch(
                "INSERT INTO lanes (name, key, last_number, position, project, author)
                     VALUES ('Tandem', 'TAN', 7, 0, 'Tandem', 'human'),
                            ('Ui', 'UI', 2, 1, 'Tandem', 'human');
                 INSERT INTO tickets
                    (id, title, status, archived, lane, number, origin, assignee, position,
                     created_at, completed_at, author)
                 VALUES
                    ('00000000-0000-0000-0000-0000000000a1', 'first', 'ready', 0, 'Tandem', 6,
                     'human', NULL, 0, '2026-10-30T00:00:00Z', NULL, 'human'),
                    ('00000000-0000-0000-0000-0000000000a2', 'second', 'ready', 0, 'Tandem', 7,
                     'human', NULL, 1, '2026-10-30T00:00:00Z', NULL, 'human'),
                    ('00000000-0000-0000-0000-0000000000a3', 'third', 'backlog', 0, 'Ui', 2,
                     'human', NULL, 0, '2026-10-30T00:00:00Z', NULL, 'human'),
                    ('00000000-0000-0000-0000-0000000000a4', 'loose', 'backlog', 0, NULL, NULL,
                     'human', NULL, 0, '2026-10-30T00:00:00Z', NULL, 'human');",
            )
            .unwrap();
        drop(connection);

        // A read before the first write still answers on the v11 shape.
        let board = Board::with_actor(sandbox.store(), Actor::Human);
        let before: Vec<(String, Option<String>)> = board
            .list(&TicketQuery::default())
            .expect("a v11 home reads before it migrates")
            .into_iter()
            .map(|ticket| (ticket.title, ticket.reference))
            .collect();
        assert_eq!(
            version(&sandbox.connection()).unwrap(),
            Some(11),
            "a read never migrates (ADR 0031)"
        );

        probe(&sandbox.store(), &Actor::Human, "migrates");

        let connection = sandbox.connection();
        assert_eq!(version(&connection).unwrap(), Some(CURRENT_VERSION));
        assert!(
            !has_table(&connection, "lanes").unwrap(),
            "lanes is renamed"
        );
        assert!(has_table(&connection, "labels").unwrap(), "labels exists");
        assert!(
            has_column(&connection, "tickets", "label").unwrap()
                && !has_column(&connection, "tickets", "lane").unwrap(),
            "tickets.lane is tickets.label"
        );
        assert!(has_index(&connection, "tickets_label_position"));
        assert!(!has_index(&connection, "tickets_lane_position"));

        let labels: Vec<(String, String, i64, i64, String)> = connection
            .prepare(
                "SELECT name, key, last_number, position, project FROM labels ORDER BY position",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(
            labels,
            vec![
                ("Tandem".into(), "TAN".into(), 7, 0, "Tandem".into()),
                ("Ui".into(), "UI".into(), 2, 1, "Tandem".into()),
            ],
            "names, keys, counters, order and project parents are unchanged"
        );
        let tickets: Vec<(String, Option<String>, Option<i64>, i64)> = connection
            .prepare("SELECT id, label, number, position FROM tickets WHERE title != 'migrates' ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(tickets.len(), 4);
        assert_eq!(tickets[0].1.as_deref(), Some("Tandem"));
        assert_eq!(tickets[2].1.as_deref(), Some("Ui"));
        assert_eq!(tickets[3].1, None, "the default area stays unnamed");
        assert_eq!(
            (tickets[0].2, tickets[1].2, tickets[2].2),
            (Some(6), Some(7), Some(2))
        );

        let after: Vec<(String, Option<String>)> = Board::with_actor(sandbox.store(), Actor::Human)
            .list(&TicketQuery::default())
            .unwrap()
            .into_iter()
            .filter(|ticket| ticket.title != "migrates")
            .map(|ticket| (ticket.title, ticket.reference))
            .collect();
        assert_eq!(
            before, after,
            "every ref reads the same across the migration"
        );
    }

    #[test]
    fn a_v6_file_reads_an_empty_board_and_the_first_write_migrates_it() {
        use crate::domain::{AddTicket, Board};

        let sandbox = Sandbox::new("read-v6");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..6], &Actor::Human).expect("v6 applies");
        drop(connection);

        let store = sandbox.store();
        let board = Board::with_actor(store, Actor::Human);
        assert!(
            board
                .list(&crate::domain::TicketQuery::default())
                .expect("the board reads")
                .is_empty(),
            "a v6 home reads instead of failing"
        );
        assert_eq!(
            version(&sandbox.connection()).unwrap(),
            Some(6),
            "a read never migrates (ADR 0031)"
        );

        board
            .add("probe", AddTicket::default())
            .expect("the first write migrates and lands the ticket");
        assert_eq!(
            version(&sandbox.connection()).unwrap(),
            Some(CURRENT_VERSION),
            "the write takes the store to the current version"
        );
    }

    #[test]
    fn an_unreadable_database_fails_the_write() {
        let sandbox = Sandbox::new("not-a-database");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        fs::write(sandbox.database(), b"this is not a database").unwrap();

        let outcome = sandbox.store().write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "k",
                position: 0,
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
        probe(&store, &Actor::Human, "seed");

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
        probe(&store, &Actor::Human, "next");
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
        probe(&store, &Actor::Human, "seed");

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
            write.insert_project(NewProject {
                name: "blocked",
                position: 0,
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
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1, "the timed-out write wrote nothing");
    }

    #[test]
    fn busy_timeout_surfaces_the_locked_error_after_the_window() {
        let sandbox = Sandbox::new("busy");
        let store = sandbox.store();
        probe(&store, &Actor::Human, "seed");

        // Another connection holds the write side for longer than the store's
        // window: the engine's contention becomes the store's "locked".
        let holder = Connection::open(sandbox.database()).unwrap();
        holder.execute_batch("BEGIN IMMEDIATE").unwrap();

        let started = Instant::now();
        let outcome = store.write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "blocked",
                position: 0,
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
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1, "the timed-out write wrote nothing");
    }

    /// The default workflow's stage keys, in position order.
    fn stage_keys(connection: &Connection) -> Vec<String> {
        let mut statement = connection
            .prepare("SELECT key FROM workflow_stages WHERE workflow_id = ?1 ORDER BY position")
            .expect("the stage read prepares");
        statement
            .query_map([super::super::schema::DEFAULT_WORKFLOW_ID], |row| {
                row.get::<_, String>(0)
            })
            .expect("the stage read runs")
            .map(|row| row.expect("a stage key"))
            .collect()
    }

    /// The six the v7 seed writes, in the foundation's order.
    const FOUNDATION_STAGES: [&str; 6] = [
        "backlog",
        "on-hold",
        "ready",
        "in-progress",
        "in-review",
        "done",
    ];

    /// TAN-327: the foundation's seeded default gains `selected` immediately
    /// before `ready`. The v7 seed stays the six, and the new migration is the
    /// only thing that adds the seventh, so a home that applied v7 keeps its
    /// history and an existing home at v7 upgrades with its tickets untouched.
    #[test]
    fn the_migration_inserts_selected_before_ready_on_a_foundation_home() {
        let sandbox = Sandbox::new("selected-migration");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..7], &Actor::Human).expect("the foundation applies");

        assert_eq!(
            stage_keys(&connection),
            FOUNDATION_STAGES.to_vec(),
            "the v7 seed stays six stages"
        );

        migrate_with(&connection, &MIGRATIONS[..9], &Actor::Human).expect("the rest applies");

        assert_eq!(
            stage_keys(&connection),
            vec![
                "backlog",
                "on-hold",
                "selected",
                "ready",
                "in-progress",
                "in-review",
                "done"
            ],
            "selected lands immediately before ready, and every other stage stays put"
        );
        let (position, label, role, binding): (i64, String, String, Option<String>) = connection
            .query_row(
                "SELECT position, label, role, binding_role FROM workflow_stages
                 WHERE workflow_id = ?1 AND key = 'selected'",
                [super::super::schema::DEFAULT_WORKFLOW_ID],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("the new stage reads");
        assert_eq!(position, 2, "it sits between on-hold and ready");
        assert_eq!(label, "Selected for development");
        assert_eq!(role, "intake");
        assert_eq!(binding, None, "the new stage carries no binding");
    }

    /// TAN-327: a customized default is the human's, never rewritten. A key set
    /// that is not exactly the six seeded keys blocks the insert.
    #[test]
    fn the_migration_leaves_a_customized_workflow_untouched() {
        let sandbox = Sandbox::new("selected-customized");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..7], &Actor::Human).expect("the foundation applies");

        connection
            .execute(
                "UPDATE workflow_stages SET key = 'parked'
                 WHERE workflow_id = ?1 AND key = 'on-hold'",
                [super::super::schema::DEFAULT_WORKFLOW_ID],
            )
            .expect("the default is customized");

        migrate_with(&connection, &MIGRATIONS[..9], &Actor::Human).expect("the rest applies");

        let keys = stage_keys(&connection);
        assert!(
            !keys.contains(&"selected".to_string()),
            "a customized key set is never migrated: {keys:?}"
        );
        assert!(keys.contains(&"parked".to_string()), "{keys:?}");
    }

    /// TAN-327: a default that already carries `selected` is left alone, so a
    /// home that applied the migration twice (or carried the stage before it)
    /// ends with one `selected`, never two.
    #[test]
    fn the_migration_is_idempotent_when_selected_exists() {
        let sandbox = Sandbox::new("selected-idempotent");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..7], &Actor::Human).expect("the foundation applies");

        connection
            .execute(
                "INSERT INTO workflow_stages
                    (workflow_id, position, key, label, role, binding_role, binding_pass)
                 VALUES (?1, 1, 'selected', 'Selected for development', 'intake', NULL, NULL)",
                [super::super::schema::DEFAULT_WORKFLOW_ID],
            )
            .expect("the stage pre-exists");

        migrate_with(&connection, &MIGRATIONS[..9], &Actor::Human).expect("the rest applies");

        let keys = stage_keys(&connection);
        assert_eq!(
            keys.iter().filter(|key| *key == "selected").count(),
            1,
            "the insert is idempotent: {keys:?}"
        );
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
            write.insert_project(NewProject {
                name: "k",
                position: 0,
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
        let message = error.to_string();
        assert!(
            message.contains("ADR 0031"),
            "the one error points at the record, not a path the tree lost: {message}"
        );
        assert!(
            !message.contains("Scripts/"),
            "the one error sends no reader to a path: {message}"
        );
    }

    fn newer_file(sandbox: &Sandbox) -> i64 {
        sandbox
            .store()
            .write(&Actor::Human, |write| {
                write.insert_project(NewProject {
                    name: "k",
                    position: 0,
                })
            })
            .expect("the build writes its own store");
        let ahead = CURRENT_VERSION + 1;
        Connection::open(sandbox.database())
            .expect("the store opens")
            .execute(
                "INSERT INTO schema_migrations (version, applied_at, author) VALUES (?1, '2026-11-16T00:00:00Z', 'newer')",
                [ahead],
            )
            .expect("the newer version is stamped");
        ahead
    }

    #[test]
    fn a_newer_store_is_refused_by_a_read_and_a_write_and_says_upgrade() {
        let sandbox = Sandbox::new("refuse-newer");
        let ahead = newer_file(&sandbox);
        let expected = StoreError::Newer {
            store: ahead,
            build: CURRENT_VERSION,
        };

        let read = sandbox.store().read(|connection| {
            connection
                .query_row("SELECT COUNT(*) FROM tickets", [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(|error| StoreError::Io(error.to_string()))
        });
        assert_eq!(read.unwrap_err(), expected);

        let write = sandbox.store().write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "j",
                position: 1,
            })
        });
        let error = write.expect_err("a newer store is not written");
        assert_eq!(error, expected);
        assert_eq!(
            error.to_string(),
            format!(
                "this home's tandem.db is schema v{ahead}; this build reads up to v{CURRENT_VERSION}. \
                 The store was written by a newer Tandem. Upgrade this binary (run the installer \
                 again), and restart `tandem serve`."
            )
        );
        assert!(!error.to_string().contains("ADR"));
        assert!(matches!(
            sandbox.store().health(),
            Health::Newer { store, build } if store == ahead && build == CURRENT_VERSION
        ));
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
            write.insert_project(NewProject {
                name: "k",
                position: 0,
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
                write.insert_label(NewLabel {
                    name: "Port",
                    key: "PRT",
                    last_number: 4,
                    position: 0,
                    project: "Port",
                })?;
                write.insert_ticket(NewTicket {
                    id: "00000000-0000-0000-0000-000000000004",
                    title: "the store adapter",
                    description: Some("the body"),
                    status: "backlog",
                    archived: false,
                    label: Some("Port"),
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
                write.insert_project(NewProject {
                    name: "defaultLabel",
                    position: 0,
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
        probe(&store, &Actor::Human, "other");
        let projects = author_of(&connection, "projects");
        assert_eq!(projects, vec!["builder".to_string(), "human".to_string()]);
        assert_eq!(
            author_of(&connection, "tickets"),
            vec!["builder".to_string()],
            "a later write does not rewrite a row it did not touch"
        );
    }
}

// MARK: - The rolling backup and concurrency

mod writes {
    use std::thread;

    use super::*;

    /// A loaded runner can spend more than the production 2 s on one write (a
    /// Defender scan of the fsync, a slow VM), and the gate's window bounds one
    /// writer's wait for the writer ahead of it. The contention case configures
    /// its own wider window; `BUSY_TIMEOUT` stays 2 s, and
    /// `gate::a_configured_gate_window_still_bites_after_a_deliberate_hold`
    /// proves a configured window still bites.
    const SLOW_RUNNER_WINDOW: Duration = Duration::from_secs(60);

    #[test]
    fn the_first_write_has_no_backup() {
        let sandbox = Sandbox::new("backup-first");
        probe(&sandbox.store(), &Actor::Human, "one");

        assert!(
            !sandbox.backup().exists(),
            "the first write has no file to roll"
        );
    }

    #[test]
    fn the_second_writes_backup_holds_the_state_before_it() {
        let sandbox = Sandbox::new("backup-second");
        let store = sandbox.store();
        probe(&store, &Actor::Human, "one");
        probe(&store, &Actor::Human, "two");

        let backup =
            Connection::open_with_flags(sandbox.backup(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .expect("the backup opens");
        let mut statement = backup
            .prepare("SELECT name FROM projects ORDER BY rowid")
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
        probe(&store, &Actor::Human, "one");

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
        probe(&store, &Actor::Human, "one");
        probe(&store, &Actor::Human, "two");
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
        probe(&store, &Actor::Human, "one");

        // A writer killed mid-backup leaves its same-directory temp: nothing
        // renames it and no read removes it, so the next write sweeps it.
        let killed = sandbox.home_path().join(".tandem-backup-4242-1-0.tmp");
        fs::write(&killed, b"half a copy").unwrap();
        aged(&killed, 120);
        // A temp young enough to belong to a writer still running is not a
        // kill's, and the sweep must leave it alone.
        let in_flight = sandbox.home_path().join(".tandem-backup-4243-2-0.tmp");
        fs::write(&in_flight, b"a copy in progress").unwrap();

        probe(&store, &Actor::Human, "two");

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
        probe(&store, &Actor::Human, "one");
        probe(&store, &Actor::Human, "two");

        // The backup's destination is a directory, so the atomic rename cannot
        // land it: the write must fail whole and change nothing.
        fs::remove_file(sandbox.backup()).unwrap();
        fs::create_dir(sandbox.backup()).unwrap();

        let outcome = store.write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "three",
                position: 0,
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
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2, "the failed write changed nothing");

        fs::remove_dir(sandbox.backup()).unwrap();
        probe(&store, &Actor::Human, "three");
        let count: i64 = sandbox
            .connection()
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
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
                write.insert_project(NewProject {
                    name: "k",
                    position: 0,
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
        // The only thing these two threads contend for is the process-local
        // write gate (`store::write_gate`), one mutex per canonical home, so
        // `BEGIN IMMEDIATE` never sees a same-process sibling and the gate's
        // window is the only wait. That window is the production 2 s by
        // default, which a loaded Windows runner outlasts with one slow write;
        // this case gives its own store a wider one. No ordering between the
        // threads is assumed: the count is read after the scope joins both.
        let store = sandbox.store().with_gate_timeout(SLOW_RUNNER_WINDOW);
        probe(&store, &Actor::Human, "seed");
        thread::scope(|scope| {
            for group in ["a", "b"] {
                let store = &store;
                scope.spawn(move || {
                    for index in 0..10 {
                        let key = format!("{group}-{index}");
                        let started = Instant::now();
                        store
                            .write(&Actor::Agent(group.to_string()), |write| {
                                write.insert_project(NewProject {
                                    name: &key,
                                    position: 0,
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
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 21, "no concurrent writer lost a row");
    }

    #[test]
    fn a_lock_free_reader_sees_a_consistent_snapshot_while_a_writer_works() {
        let sandbox = Sandbox::new("snapshot");
        let store = sandbox.store();
        probe(&store, &Actor::Human, "seed");

        let mut last_seen = 1;
        let mut reads = 0;
        thread::scope(|scope| {
            let writer = scope.spawn(|| {
                for index in 0..20 {
                    let key = format!("w-{index}");
                    store
                        .write(&Actor::Human, |write| {
                            write.insert_project(NewProject {
                                name: &key,
                                position: 0,
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
                            .query_row("SELECT COUNT(*) FROM projects", [], |row| {
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
                    .query_row("SELECT COUNT(*) FROM projects", [], |row| {
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
// pipe suite in `cli/tests/rpc.rs` covers the same shape through the child.
#[cfg(unix)]
#[test]
fn a_restored_store_is_watched_without_reopening() {
    let sandbox = Sandbox::new("watch-restore");
    let store = sandbox.store();
    probe(&store, &Actor::Human, "one");
    probe(&store, &Actor::Human, "two");

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
    probe(&store, &Actor::Human, "three");
    assert!(restored.revision().expect("the watch reads") > revision);
}

// The restore's other half, and the only half Windows can run: the app is
// closed for the restore (the documented recipe says so on Windows),
// so nothing is renamed over a held file. The store the restore leaves is the
// rollback-journal backup copied into place, and the watch has to attach to it
// and answer its revision. Copying the backup into a home of its own produces
// exactly that file without the rename the platform forbids.
#[test]
fn a_rollback_journal_store_answers_a_revision_to_the_watch() {
    let sandbox = Sandbox::new("watch-rollback-journal");
    let store = sandbox.store();
    probe(&store, &Actor::Human, "one");
    probe(&store, &Actor::Human, "two");
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
    probe(&restored, &Actor::Human, "three");
    assert!(watch.revision().expect("the watch reads") > revision);
}

// MARK: - The archive's copy (CUT-35)

mod export_copy {
    use super::*;
    use crate::store::export::{CARRIED_TABLES, EMPTIED_TABLES, current_tables, scrub};

    #[test]
    fn every_table_of_the_current_schema_is_classified_carried_or_emptied() {
        for table in current_tables() {
            let carried = CARRIED_TABLES.contains(&table.as_str());
            let emptied = EMPTIED_TABLES.contains(&table.as_str());
            assert!(
                carried || emptied,
                "the table `{table}` is in neither list: classify it in store/export.rs, and a \
                 table of secrets (a sign-in session, a token digest) goes in EMPTIED_TABLES"
            );
            assert!(!(carried && emptied), "`{table}` is in both lists");
        }
        for table in EMPTIED_TABLES {
            assert!(
                !CARRIED_TABLES.contains(table),
                "`{table}` is in both lists"
            );
        }
    }

    #[test]
    fn the_export_copy_is_scrubbed_and_vacuumed_so_no_emptied_row_survives_in_the_file() {
        let sandbox = Sandbox::new("scrub");
        let path = sandbox.root().join("copy.db");
        const MARKER: &str = "SECRET-MARKER-9f3a1c";
        {
            let connection = Connection::open(&path).expect("the copy opens");
            connection
                .execute_batch(
                    "CREATE TABLE board (note TEXT); CREATE TABLE browser_sessions (token TEXT);",
                )
                .unwrap();
            for index in 0..200 {
                connection
                    .execute(
                        "INSERT INTO browser_sessions (token) VALUES (?1)",
                        [format!("{MARKER}-{index}")],
                    )
                    .unwrap();
            }
            connection
                .execute("INSERT INTO board (note) VALUES ('kept')", [])
                .unwrap();
        }
        assert!(contains(&fs::read(&path).unwrap(), MARKER.as_bytes()));

        scrub(&path, &["board"], &["browser_sessions"]).expect("the scrub runs");

        let bytes = fs::read(&path).unwrap();
        assert!(
            !contains(&bytes, MARKER.as_bytes()),
            "an emptied row survives in the file's free pages"
        );
        let connection = Connection::open(&path).unwrap();
        let rows = |table: &str| -> i64 {
            connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(rows("browser_sessions"), 0);
        assert_eq!(rows("board"), 1);
        let mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "delete");
        assert!(!path.with_extension("db-wal").exists());

        // A table nobody classified fails the scrub: the export fails closed.
        connection
            .execute_batch("CREATE TABLE stray (x TEXT)")
            .unwrap();
        drop(connection);
        let error =
            scrub(&path, &["board"], &["browser_sessions"]).expect_err("a stray table fails");
        assert!(error.to_string().contains("stray"), "{error}");
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    /// The member tables (CUT-36, schema v14): the names are carried, the
    /// credentials' digests are emptied, and every one of them is classified.
    #[test]
    fn the_member_tables_are_classified_for_the_archive_members_carried_credentials_emptied() {
        assert!(CARRIED_TABLES.contains(&"members"));
        assert!(EMPTIED_TABLES.contains(&"member_tokens"));
        assert!(EMPTIED_TABLES.contains(&"member_signin_keys"));
        assert!(EMPTIED_TABLES.contains(&"browser_sessions"));
        let sandbox = Sandbox::new("member-classes");
        let store = sandbox.store();
        probe(&store, &Actor::Human, "first");
        store
            .member_add("alice", Role::Admin, 1_800_000_000)
            .unwrap();
        let token = store
            .member_token_issue("alice", "laptop", 1_800_000_000)
            .unwrap()
            .expect("a live member gets a token");
        let key = store
            .member_signin_key_issue("alice", 1_800_000_000)
            .unwrap()
            .expect("a live member gets a key");
        let tables = current_tables();
        for table in ["members", "member_tokens", "member_signin_keys"] {
            assert!(tables.iter().any(|name| name == table), "{table} exists");
        }
        let copy = store.export_copy().expect("the copy is taken");
        let bytes = fs::read(copy.path()).unwrap();
        let connection =
            Connection::open_with_flags(copy.path(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let rows = |table: &str| -> i64 {
            connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(rows("members"), 1, "the member's name and role travel");
        assert_eq!(rows("member_tokens"), 0);
        assert_eq!(rows("member_signin_keys"), 0);
        for digest in [
            credential_digest(token.reveal()),
            credential_digest(key.reveal()),
        ] {
            assert!(
                !contains(&bytes, digest.as_bytes()),
                "no credential digest survives in the archive's file"
            );
        }
    }

    #[test]
    fn taking_the_export_copy_moves_neither_the_revision_nor_the_rolling_backup() {
        let sandbox = Sandbox::new("copy-quiet");
        let store = sandbox.store();
        let actor = Actor::Human;
        probe(&store, &actor, "one");
        probe(&store, &actor, "two");
        let revision = store.revision().unwrap();
        let backup = fs::read(sandbox.backup()).expect("the rolling backup exists");
        let backup_time = fs::metadata(sandbox.backup()).unwrap().modified().unwrap();
        let database = fs::read(sandbox.database()).unwrap();

        let copy = store.export_copy().expect("the copy is taken");
        assert!(copy.path().exists());
        assert_eq!(copy.schema_version(), CURRENT_VERSION);
        let path = copy.path().to_path_buf();
        drop(copy);
        assert!(!path.exists(), "the copy is removed when its guard drops");

        assert_eq!(store.revision().unwrap(), revision);
        assert_eq!(fs::read(sandbox.backup()).unwrap(), backup);
        assert_eq!(
            fs::metadata(sandbox.backup()).unwrap().modified().unwrap(),
            backup_time
        );
        assert_eq!(fs::read(sandbox.database()).unwrap(), database);
    }
}

mod browser_sessions {
    use super::super::schema::{
        BROWSER_SESSIONS_VERSION, CURRENT_VERSION, MIGRATIONS, has_table, migrate_with, version,
    };
    use super::*;

    const DAY: i64 = 24 * 60 * 60;
    const NOW: i64 = 1_800_000_000;

    fn rows(sandbox: &Sandbox) -> Vec<(String, i64, i64)> {
        let connection = sandbox.connection();
        let mut statement = connection
            .prepare(
                "SELECT digest, created_at, last_used_at FROM browser_sessions ORDER BY digest",
            )
            .unwrap();
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .unwrap()
            .map(|row| row.unwrap())
            .collect()
    }

    #[test]
    fn the_browser_sessions_migration_adds_the_table_and_bumps_the_version() {
        let sandbox = Sandbox::new("sessions-migration");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..12], &Actor::Human).expect("v12 applies");
        assert!(!has_table(&connection, "browser_sessions").unwrap());
        drop(connection);

        probe(&sandbox.store(), &Actor::Human, "migrates");

        let connection = sandbox.connection();
        assert_eq!(version(&connection).unwrap(), Some(CURRENT_VERSION));
        assert_eq!(BROWSER_SESSIONS_VERSION, 13);
        assert!(has_table(&connection, "browser_sessions").unwrap());
        assert!(
            !has_table(&connection, "sessions").unwrap(),
            "the retired name stays retired"
        );
    }

    #[test]
    fn session_writes_do_not_move_the_revision_or_arm_the_backup() {
        let sandbox = Sandbox::new("sessions-revision");
        let store = sandbox.store();
        probe(&store, &Actor::Human, "first");
        probe(&store, &Actor::Human, "second");
        let revision = store.revision().unwrap();
        let backup_before = fs::read(sandbox.backup()).ok();
        let backup_modified = fs::metadata(sandbox.backup())
            .and_then(|m| m.modified())
            .ok();

        store.browser_session_open("aa", None, NOW).unwrap();
        store.browser_session_open("bb", Some("aa"), NOW).unwrap();
        store
            .browser_session_check("bb", NOW + 2 * DAY, true)
            .unwrap();
        store.browser_sessions_sweep(NOW + 3 * DAY).unwrap();
        store.browser_session_close("bb").unwrap();

        assert_eq!(store.revision().unwrap(), revision, "no revision bump");
        assert_eq!(
            fs::read(sandbox.backup()).ok(),
            backup_before,
            "no backup roll"
        );
        assert_eq!(
            fs::metadata(sandbox.backup())
                .and_then(|m| m.modified())
                .ok(),
            backup_modified
        );
    }

    #[test]
    fn the_store_holds_a_digest_and_never_the_session_id() {
        let sandbox = Sandbox::new("sessions-digest");
        let store = sandbox.store();
        store.browser_session_open("the-digest", None, NOW).unwrap();
        let held = rows(&sandbox);
        assert_eq!(held, vec![("the-digest".to_string(), NOW, NOW)]);
        // The file and its backup carry the digest and nothing else a login had.
        let bytes = fs::read(sandbox.database()).unwrap();
        assert!(bytes.windows(10).any(|window| window == b"the-digest"));
        assert!(
            !has_column_named(&sandbox, "browser_sessions", "id"),
            "no id column"
        );
    }

    fn has_column_named(sandbox: &Sandbox, table: &str, column: &str) -> bool {
        super::super::schema::has_column(&sandbox.connection(), table, column).unwrap()
    }

    #[test]
    fn last_use_is_rewritten_at_most_once_a_day() {
        let sandbox = Sandbox::new("sessions-touch");
        let store = sandbox.store();
        store.browser_session_open("aa", None, NOW).unwrap();

        assert_eq!(
            store
                .browser_session_check("aa", NOW + DAY - 1, true)
                .unwrap(),
            SessionCheck::Valid,
            "inside the day: no rewrite"
        );
        assert_eq!(rows(&sandbox)[0].2, NOW);
        assert_eq!(
            store.browser_session_check("aa", NOW + DAY, false).unwrap(),
            SessionCheck::Valid,
            "a check that does not touch never rewrites"
        );
        assert_eq!(rows(&sandbox)[0].2, NOW);
        assert_eq!(
            store.browser_session_check("aa", NOW + DAY, true).unwrap(),
            SessionCheck::Refreshed
        );
        assert_eq!(rows(&sandbox)[0].2, NOW + DAY);
        assert_eq!(
            store
                .browser_session_check("aa", NOW + DAY + 5, true)
                .unwrap(),
            SessionCheck::Valid
        );
        assert_eq!(rows(&sandbox)[0].2, NOW + DAY, "once a day");
    }

    #[test]
    fn expired_sessions_are_swept() {
        let sandbox = Sandbox::new("sessions-sweep");
        let store = sandbox.store();
        store.browser_session_open("old", None, NOW).unwrap();
        store
            .browser_session_open("fresh", None, NOW + 20 * DAY)
            .unwrap();

        assert_eq!(
            store
                .browser_session_check("old", NOW + 31 * DAY, true)
                .unwrap(),
            SessionCheck::Missing,
            "idle past 30 days reads as no session"
        );
        assert_eq!(store.browser_sessions_sweep(NOW + 31 * DAY).unwrap(), 1);
        let left: Vec<String> = rows(&sandbox).into_iter().map(|row| row.0).collect();
        assert_eq!(left, vec!["fresh".to_string()]);

        // A login sweeps too.
        store
            .browser_session_open("newer", None, NOW + 60 * DAY)
            .unwrap();
        let left: Vec<String> = rows(&sandbox).into_iter().map(|row| row.0).collect();
        assert_eq!(left, vec!["newer".to_string()]);

        // A sweep never migrates a home that has no table.
        let bare = Sandbox::new("sessions-sweep-bare");
        assert_eq!(bare.store().browser_sessions_sweep(NOW).unwrap(), 0);
        assert!(!bare.database().exists(), "a sweep creates nothing");
    }
}

// MARK: - Members and credentials (CUT-36, ADR 0052)

mod members {
    use super::super::schema::{
        CURRENT_VERSION, MEMBERS_VERSION, MIGRATIONS, has_column, has_table, migrate_with, version,
    };
    use super::*;

    const DAY: i64 = 24 * 60 * 60;
    const NOW: i64 = 1_800_000_000;

    fn count(sandbox: &Sandbox, sql: &str) -> i64 {
        sandbox
            .connection()
            .query_row(sql, [], |row| row.get(0))
            .unwrap()
    }

    /// A home migrated to v13 only, as a serve built before this change left it.
    fn v13_home(name: &str) -> Sandbox {
        let sandbox = Sandbox::new(name);
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..13], &Actor::Human).expect("v13 applies");
        drop(connection);
        sandbox
    }

    #[test]
    fn the_member_migration_adds_the_tables_and_the_session_column_and_bumps_the_version() {
        let sandbox = v13_home("members-migration");
        let connection = Connection::open(sandbox.database()).unwrap();
        assert!(!has_table(&connection, "members").unwrap());
        assert!(!has_column(&connection, "browser_sessions", "member_id").unwrap());
        drop(connection);

        probe(&sandbox.store(), &Actor::Human, "migrates");

        let connection = sandbox.connection();
        assert_eq!(MEMBERS_VERSION, 14);
        assert_eq!(version(&connection).unwrap(), Some(CURRENT_VERSION));
        assert_eq!(CURRENT_VERSION, 15);
        for table in ["members", "member_tokens", "member_signin_keys"] {
            assert!(has_table(&connection, table).unwrap(), "{table}");
        }
        assert!(has_column(&connection, "browser_sessions", "member_id").unwrap());
        assert!(
            sandbox.backup().exists(),
            "a migrating write rolls the backup first"
        );
    }

    #[test]
    fn a_member_name_is_unique_forever_and_reserved_names_are_refused() {
        let sandbox = Sandbox::new("members-names");
        let store = sandbox.store();
        let alice = store.member_add("alice", Role::Admin, NOW).unwrap();
        assert_eq!(alice.name, "alice");
        assert_eq!(
            store.member_add("alice", Role::Member, NOW),
            Err(MemberError::Taken)
        );
        // The last admin cannot go, so a second one stands beside alice.
        store.member_add("zed", Role::Admin, NOW).unwrap();
        assert!(store.member_remove("alice", NOW).unwrap());
        assert_eq!(
            store.member_add("alice", Role::Member, NOW),
            Err(MemberError::Taken),
            "a name is never reused, even after removal"
        );
        assert!(store.member_find("alice").unwrap().is_none());
        for reserved in ["human", "operator", "owner"] {
            assert!(
                matches!(
                    store.member_add(reserved, Role::Member, NOW),
                    Err(MemberError::Invalid(_))
                ),
                "{reserved}"
            );
        }
        let long = "a".repeat(65);
        for bad in ["", "Alice", "a b", "a_b", long.as_str()] {
            assert!(
                matches!(
                    store.member_add(bad, Role::Member, NOW),
                    Err(MemberError::Invalid(_))
                ),
                "{bad:?}"
            );
        }
        assert_eq!(count(&sandbox, "SELECT COUNT(*) FROM members"), 2);
    }

    #[test]
    fn the_first_member_must_be_an_admin() {
        let sandbox = Sandbox::new("members-first-admin");
        let store = sandbox.store();
        assert_eq!(
            store.member_add("bob", Role::Member, NOW),
            Err(MemberError::NoAdmin),
            "a home with no admin refuses a member"
        );
        assert_eq!(count(&sandbox, "SELECT COUNT(*) FROM members"), 0);
        assert!(store.member_add("alice", Role::Admin, NOW).is_ok());
        assert!(store.member_add("bob", Role::Member, NOW).is_ok());
    }

    #[test]
    fn tokens_and_sign_in_keys_are_stored_as_digests_and_never_the_value() {
        let sandbox = Sandbox::new("members-digests");
        let store = sandbox.store();
        store.member_add("alice", Role::Admin, NOW).unwrap();
        let token = store
            .member_token_issue("alice", "laptop", NOW)
            .unwrap()
            .unwrap();
        let key = store
            .member_signin_key_issue("alice", NOW)
            .unwrap()
            .unwrap();

        let (token_value, key_value) = (token.reveal().to_string(), key.reveal().to_string());
        assert!(token_value.starts_with(TOKEN_PREFIX));
        assert!(key_value.starts_with(SIGNIN_KEY_PREFIX));
        assert_eq!(token_value.len(), 4 + 43);
        assert_eq!(key_value.len(), 4 + 43);
        assert_eq!(credential_kind(&token_value), Some(CredentialKind::Token));
        assert_eq!(credential_kind(&key_value), Some(CredentialKind::SigninKey));
        assert_ne!(
            token_value[4..],
            store
                .member_token_issue("alice", "second", NOW)
                .unwrap()
                .unwrap()
                .reveal()[4..],
            "each mint is fresh"
        );
        let printed = format!("{token:?} {key:?}");
        assert!(!printed.contains(&token_value) && printed.contains("redacted"));

        let connection = sandbox.connection();
        let stored: String = connection
            .query_row(
                "SELECT digest FROM member_tokens WHERE label = 'laptop'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, credential_digest(&token_value));
        assert_eq!(stored.len(), 64);
        let stored_key: String = connection
            .query_row("SELECT digest FROM member_signin_keys", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stored_key, credential_digest(&key_value));
        for file in [sandbox.database(), sandbox.backup()] {
            let Ok(bytes) = fs::read(file) else { continue };
            for value in [&token_value, &key_value] {
                assert!(
                    !bytes
                        .windows(value.len())
                        .any(|window| window == value.as_bytes()),
                    "the value is never in the file"
                );
            }
        }
        assert_eq!(
            store
                .member_by_token(&token_value, NOW, false)
                .unwrap()
                .map(|m| m.name),
            Some("alice".to_string())
        );
        assert_eq!(
            store
                .member_by_signin_key(&key_value)
                .unwrap()
                .map(|m| m.name),
            Some("alice".to_string())
        );
        assert!(
            store
                .member_by_token(&key_value, NOW, false)
                .unwrap()
                .is_none(),
            "a key is not a token"
        );
        assert!(store.member_by_signin_key(&token_value).unwrap().is_none());
        let newer = store
            .member_signin_key_issue("alice", NOW + 1)
            .unwrap()
            .unwrap();
        assert!(
            store.member_by_signin_key(&key_value).unwrap().is_none(),
            "one live key per member: a new one replaces the old"
        );
        assert!(
            store
                .member_by_signin_key(newer.reveal())
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn credential_writes_and_last_used_move_neither_the_revision_nor_the_rolling_backup() {
        let sandbox = Sandbox::new("members-revision");
        let store = sandbox.store();
        probe(&store, &Actor::Human, "first");
        probe(&store, &Actor::Human, "second");
        let revision = store.revision().unwrap();
        let backup_before = fs::read(sandbox.backup()).ok();
        let backup_modified = fs::metadata(sandbox.backup())
            .and_then(|m| m.modified())
            .ok();

        store.member_add("alice", Role::Admin, NOW).unwrap();
        store.member_add("bob", Role::Member, NOW).unwrap();
        let token = store
            .member_token_issue("alice", "laptop", NOW)
            .unwrap()
            .unwrap();
        store.member_signin_key_issue("alice", NOW).unwrap();
        assert!(
            store
                .member_by_token(token.reveal(), NOW, true)
                .unwrap()
                .is_some()
        );
        let first_use: Option<i64> = sandbox
            .connection()
            .query_row("SELECT last_used_at FROM member_tokens", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(first_use, Some(NOW), "the first use is recorded");
        store
            .member_by_token(token.reveal(), NOW + DAY - 1, true)
            .unwrap();
        let again: Option<i64> = sandbox
            .connection()
            .query_row("SELECT last_used_at FROM member_tokens", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(again, Some(NOW), "rewritten at most once a day");
        store
            .member_by_token(token.reveal(), NOW + DAY, true)
            .unwrap();
        let next: Option<i64> = sandbox
            .connection()
            .query_row("SELECT last_used_at FROM member_tokens", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(next, Some(NOW + DAY));
        store
            .browser_session_open_for("aa", None, NOW, Some("alice"))
            .unwrap();
        assert!(store.member_token_revoke("alice", 1, NOW).unwrap());
        store.member_remove("bob", NOW).unwrap();

        assert_eq!(store.revision().unwrap(), revision, "no revision bump");
        assert_eq!(
            fs::read(sandbox.backup()).ok(),
            backup_before,
            "no backup roll"
        );
        assert_eq!(
            fs::metadata(sandbox.backup())
                .and_then(|m| m.modified())
                .ok(),
            backup_modified
        );
    }

    #[test]
    fn removing_a_member_deletes_their_sessions_tokens_and_key_in_one_write() {
        let sandbox = Sandbox::new("members-remove");
        let store = sandbox.store();
        store.member_add("alice", Role::Admin, NOW).unwrap();
        store.member_add("bob", Role::Member, NOW).unwrap();
        store.member_add("zed", Role::Admin, NOW).unwrap();
        for name in ["alice", "bob"] {
            store.member_token_issue(name, "laptop", NOW).unwrap();
            store.member_token_issue(name, "desktop", NOW).unwrap();
            store.member_signin_key_issue(name, NOW).unwrap();
            store
                .browser_session_open_for(&format!("session-{name}"), None, NOW, Some(name))
                .unwrap();
        }
        store
            .browser_session_open("owner-session", None, NOW)
            .unwrap();

        assert!(store.member_remove("alice", NOW + 5).unwrap());
        assert!(
            !store.member_remove("alice", NOW + 6).unwrap(),
            "a second removal finds no live member"
        );

        let removed_at: Option<i64> = sandbox
            .connection()
            .query_row(
                "SELECT removed_at FROM members WHERE name = 'alice'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(removed_at, Some(NOW + 5), "the row stays as a tombstone");
        assert_eq!(
            count(
                &sandbox,
                "SELECT COUNT(*) FROM member_tokens WHERE member_id = 1"
            ),
            0
        );
        assert_eq!(
            count(
                &sandbox,
                "SELECT COUNT(*) FROM member_signin_keys WHERE member_id = 1"
            ),
            0
        );
        assert_eq!(
            count(
                &sandbox,
                "SELECT COUNT(*) FROM browser_sessions WHERE member_id = 1"
            ),
            0
        );
        assert_eq!(
            count(
                &sandbox,
                "SELECT COUNT(*) FROM member_tokens WHERE member_id = 2"
            ),
            2
        );
        assert_eq!(
            count(
                &sandbox,
                "SELECT COUNT(*) FROM member_signin_keys WHERE member_id = 2"
            ),
            1
        );
        assert_eq!(count(&sandbox, "SELECT COUNT(*) FROM browser_sessions"), 2);
        assert_eq!(
            store
                .browser_session_check("session-alice", NOW + 6, false)
                .unwrap(),
            SessionCheck::Missing
        );
        let bob = store
            .browser_session_lookup("session-bob", NOW + 6, false)
            .unwrap();
        assert_eq!(bob.check, SessionCheck::Valid);
        assert_eq!(
            bob.member.map(|member| member.name),
            Some("bob".to_string())
        );
        assert!(
            !store
                .browser_session_open_for("late", None, NOW + 7, Some("alice"))
                .unwrap(),
            "no session is minted for a removed member"
        );
    }

    #[test]
    fn a_store_without_the_member_tables_reads_as_no_members() {
        let sandbox = v13_home("members-absent");
        let store = sandbox.store();
        let token = format!("tdm_{}", "A".repeat(43));
        let key = format!("tdk_{}", "A".repeat(43));
        assert!(!store.team_mode().unwrap());
        assert!(store.members().unwrap().is_empty());
        assert!(store.member_find("alice").unwrap().is_none());
        assert!(store.member_by_token(&token, NOW, true).unwrap().is_none());
        assert!(store.member_by_signin_key(&key).unwrap().is_none());
        assert!(store.member_tokens("alice").unwrap().is_empty());
        let session = store
            .browser_session_lookup("anything", NOW, false)
            .unwrap();
        assert_eq!(
            (session.check, session.member),
            (SessionCheck::Missing, None)
        );
        let connection = sandbox.connection();
        assert_eq!(
            version(&connection).unwrap(),
            Some(13),
            "a read migrates nothing"
        );

        let bare = Sandbox::new("members-absent-bare");
        assert!(!bare.store().team_mode().unwrap());
        assert!(
            bare.store()
                .member_by_token(&token, NOW, true)
                .unwrap()
                .is_none()
        );
        assert!(!bare.database().exists(), "a read creates nothing");
    }

    #[test]
    fn an_existing_session_row_migrates_to_the_owner_session() {
        let sandbox = v13_home("members-owner-session");
        Connection::open(sandbox.database())
            .unwrap()
            .execute(
                "INSERT INTO browser_sessions (digest, created_at, last_used_at) VALUES ('old', ?1, ?1)",
                [NOW],
            )
            .unwrap();
        let store = sandbox.store();
        let before = store.browser_session_lookup("old", NOW + 1, false).unwrap();
        assert_eq!(
            (before.check, before.member),
            (SessionCheck::Valid, None),
            "a v13 row reads as an owner session before it migrates"
        );

        probe(&store, &Actor::Human, "migrates");

        let member_id: Option<i64> = sandbox
            .connection()
            .query_row(
                "SELECT member_id FROM browser_sessions WHERE digest = 'old'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(member_id, None);
        let after = store.browser_session_lookup("old", NOW + 1, false).unwrap();
        assert_eq!((after.check, after.member), (SessionCheck::Valid, None));
    }
}

// MARK: - The access record and the last-admin rule (CUT-39)

mod access {
    use super::super::schema::{
        ACCESS_RECORD_VERSION, CURRENT_VERSION, MIGRATIONS, has_table, migrate_with, version,
    };
    use super::*;
    use crate::store::export::CARRIED_TABLES;

    const NOW: i64 = 1_800_000_000;

    fn count(sandbox: &Sandbox, sql: &str) -> i64 {
        sandbox
            .connection()
            .query_row(sql, [], |row| row.get(0))
            .unwrap()
    }

    fn admins(sandbox: &Sandbox) -> i64 {
        count(
            sandbox,
            "SELECT COUNT(*) FROM members WHERE role = 'admin' AND removed_at IS NULL",
        )
    }

    #[test]
    fn the_last_admin_cannot_be_removed_or_demoted() {
        let sandbox = Sandbox::new("access-last-admin");
        let store = sandbox.store();
        store.member_add("alice", Role::Admin, NOW).unwrap();
        store.member_add("bob", Role::Member, NOW).unwrap();
        let rows = store.access_record().unwrap().len();

        // No door gets past it: the owner, a member, and the operator.
        for by in [
            AccessActor::owner(),
            AccessActor::member_session("alice"),
            AccessActor::operator(),
        ] {
            assert_eq!(
                store.member_remove_by("alice", NOW + 1, &by),
                Err(MemberError::LastAdmin),
                "{by:?}"
            );
            assert_eq!(
                store.member_set_role_by("alice", Role::Member, NOW + 1, &by),
                Err(MemberError::LastAdmin),
                "{by:?}"
            );
        }
        assert_eq!(
            store.member_remove("alice", NOW + 1),
            Err(MemberError::LastAdmin)
        );
        assert_eq!(admins(&sandbox), 1);
        assert_eq!(
            store.access_record().unwrap().len(),
            rows,
            "a refused act writes no row"
        );

        // A member is not an admin: removing or demoting them is free.
        assert_eq!(
            store.member_set_role("bob", Role::Member, NOW),
            Ok(true),
            "the role is already member: nothing to change, nothing recorded"
        );
        assert_eq!(store.member_remove("bob", NOW + 2), Ok(true));

        // With a second admin the first may go, and then the second is last.
        store.member_add("carol", Role::Admin, NOW).unwrap();
        assert_eq!(
            store.member_set_role_by("alice", Role::Member, NOW + 3, &AccessActor::operator()),
            Ok(true)
        );
        assert_eq!(
            store.member_remove("carol", NOW + 4),
            Err(MemberError::LastAdmin)
        );
        assert_eq!(admins(&sandbox), 1);
    }

    #[test]
    fn two_admins_demoting_each_other_at_once_leave_one() {
        let sandbox = Sandbox::new("access-race");
        let store = sandbox.store();
        store.member_add("alice", Role::Admin, NOW).unwrap();
        store.member_add("bob", Role::Admin, NOW).unwrap();

        for round in 0..2 {
            let results: Vec<_> = if round == 0 {
                // Each demotes the other.
                let (a, b) = (sandbox.store(), sandbox.store());
                let first = std::thread::spawn(move || {
                    a.member_set_role_by(
                        "bob",
                        Role::Member,
                        NOW + 1,
                        &AccessActor::member_session("alice"),
                    )
                });
                let second = std::thread::spawn(move || {
                    b.member_set_role_by(
                        "alice",
                        Role::Member,
                        NOW + 1,
                        &AccessActor::member_session("bob"),
                    )
                });
                vec![first.join().unwrap(), second.join().unwrap()]
            } else {
                // The same race with removals, on a fresh pair.
                let sandbox = Sandbox::new("access-race-remove");
                let store = sandbox.store();
                store.member_add("carol", Role::Admin, NOW).unwrap();
                store.member_add("dave", Role::Admin, NOW).unwrap();
                let (a, b) = (sandbox.store(), sandbox.store());
                let first = std::thread::spawn(move || a.member_remove("dave", NOW + 1));
                let second = std::thread::spawn(move || b.member_remove("carol", NOW + 1));
                let results = vec![first.join().unwrap(), second.join().unwrap()];
                assert_eq!(admins(&sandbox), 1, "removals leave one admin");
                results
            };
            let landed = results.iter().filter(|result| **result == Ok(true)).count();
            let refused = results
                .iter()
                .filter(|result| **result == Err(MemberError::LastAdmin))
                .count();
            assert_eq!((landed, refused), (1, 1), "round {round}: {results:?}");
        }
        assert_eq!(admins(&sandbox), 1, "demotions leave one admin");
    }

    #[test]
    fn an_admin_may_remove_or_demote_themselves_when_another_admin_exists() {
        let sandbox = Sandbox::new("access-self");
        let store = sandbox.store();
        store.member_add("alice", Role::Admin, NOW).unwrap();
        store.member_add("bob", Role::Admin, NOW).unwrap();
        store.member_add("carol", Role::Admin, NOW).unwrap();
        for name in ["alice", "carol"] {
            store
                .browser_session_open_for(&format!("session-{name}"), None, NOW, Some(name))
                .unwrap();
        }

        // Alice demotes herself: her sessions end with the change.
        assert_eq!(
            store.member_set_role_by(
                "alice",
                Role::Member,
                NOW + 1,
                &AccessActor::member_session("alice")
            ),
            Ok(true)
        );
        assert_eq!(
            store
                .browser_session_check("session-alice", NOW + 2, false)
                .unwrap(),
            SessionCheck::Missing,
            "a self-demotion ends the member's own sessions"
        );
        assert_eq!(
            store
                .browser_session_check("session-carol", NOW + 2, false)
                .unwrap(),
            SessionCheck::Valid
        );

        // Carol removes herself while bob stands: allowed, and she is gone.
        assert_eq!(
            store.member_remove_by("carol", NOW + 3, &AccessActor::member_session("carol")),
            Ok(true)
        );
        assert!(store.member_find("carol").unwrap().is_none());
        // Bob is now the last admin, and the rule holds again.
        assert_eq!(
            store.member_remove_by("bob", NOW + 4, &AccessActor::member_session("bob")),
            Err(MemberError::LastAdmin)
        );
    }

    #[test]
    fn the_access_record_row_and_the_act_commit_together_or_not_at_all() {
        let sandbox = Sandbox::new("access-atomic");
        let store = sandbox.store();
        store.member_add("alice", Role::Admin, NOW).unwrap();
        assert_eq!(store.access_record().unwrap().len(), 1);

        // An act that cannot be recorded does not happen: the insert into the
        // record fails, so the member row rolls back with it.
        Connection::open(sandbox.database())
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER block_the_record BEFORE INSERT ON access_record
                 BEGIN SELECT RAISE(ABORT, 'no room'); END;",
            )
            .unwrap();
        assert!(store.member_add("bob", Role::Member, NOW).is_err());
        assert!(store.member_token_issue("alice", "laptop", NOW).is_err());
        assert!(store.member_signin_key_issue("alice", NOW).is_err());
        assert!(store.member_find("bob").unwrap().is_none(), "no change");
        assert_eq!(count(&sandbox, "SELECT COUNT(*) FROM member_tokens"), 0);
        assert_eq!(
            count(&sandbox, "SELECT COUNT(*) FROM member_signin_keys"),
            0
        );
        assert_eq!(count(&sandbox, "SELECT COUNT(*) FROM access_record"), 1);
        Connection::open(sandbox.database())
            .unwrap()
            .execute_batch("DROP TRIGGER block_the_record")
            .unwrap();

        // The converse: an act that does not happen leaves no row.
        let rows = store.access_record().unwrap().len();
        assert_eq!(
            store.member_add("alice", Role::Member, NOW),
            Err(MemberError::Taken)
        );
        assert_eq!(store.member_remove("ghost", NOW), Ok(false));
        assert!(
            store
                .member_token_issue("ghost", "x", NOW)
                .unwrap()
                .is_none()
        );
        assert!(!store.member_token_revoke("alice", 99, NOW).unwrap());
        assert_eq!(store.access_record().unwrap().len(), rows);

        // And an act that happens leaves exactly one row.
        store.member_add("bob", Role::Member, NOW + 1).unwrap();
        assert_eq!(store.access_record().unwrap().len(), rows + 1);
    }

    #[test]
    fn the_access_record_holds_no_secret_and_has_no_update_or_delete_path() {
        let sandbox = Sandbox::new("access-secrets");
        let store = sandbox.store();
        store.member_add("alice", Role::Admin, NOW).unwrap();
        store.member_add("bob", Role::Member, NOW).unwrap();
        let token = store
            .member_token_issue_by("bob", "laptop", NOW, &AccessActor::member_session("bob"))
            .unwrap()
            .unwrap();
        let key = store
            .member_signin_key_issue_by("bob", NOW, &AccessActor::member_session("alice"))
            .unwrap()
            .unwrap();
        store
            .member_set_role_by(
                "bob",
                Role::Admin,
                NOW + 1,
                &AccessActor::member_session("alice"),
            )
            .unwrap();
        store.member_token_revoke("bob", 1, NOW + 2).unwrap();
        store
            .access_record_export(&AccessActor::owner(), "board", NOW + 3)
            .unwrap();
        store.member_remove("bob", NOW + 4).unwrap();

        let rows = store.access_record().unwrap();
        let events: Vec<&str> = rows.iter().map(|row| row.event.as_str()).collect();
        assert_eq!(
            events,
            [
                "member-added",
                "member-added",
                "token-created",
                "signin-key-issued",
                "member-role-changed",
                "token-revoked",
                "export",
                "member-removed",
            ]
        );
        let everything = format!("{rows:?}");
        let file = fs::read(sandbox.database()).unwrap();
        for secret in [
            token.reveal().to_string(),
            key.reveal().to_string(),
            credential_digest(token.reveal()),
            credential_digest(key.reveal()),
        ] {
            assert!(!everything.contains(&secret), "no secret in a row");
        }
        let exported = rows.iter().find(|row| row.event == "export").unwrap();
        assert_eq!(
            (
                exported.actor.as_str(),
                exported.credential.as_str(),
                exported.door.as_deref()
            ),
            ("owner", "owner", Some("board"))
        );
        let created = &rows[2];
        assert_eq!(created.actor, "bob");
        assert_eq!(created.credential, "member-session");
        assert_eq!(created.target.as_deref(), Some("bob"));
        assert_eq!(created.detail.as_deref(), Some("laptop"));
        drop(file);

        // No path edits a row: the triggers abort a raw statement, and no
        // store source holds one.
        let connection = sandbox.connection();
        assert!(
            connection
                .execute("UPDATE access_record SET actor = 'x'", [])
                .is_err()
        );
        assert!(connection.execute("DELETE FROM access_record", []).is_err());
        for source in [
            include_str!("members.rs"),
            include_str!("access_record.rs"),
            include_str!("browser_sessions.rs"),
            include_str!("export.rs"),
            include_str!("write.rs"),
            include_str!("board_rows.rs"),
        ] {
            let lowered = source.to_lowercase();
            assert!(!lowered.contains("update access_record"));
            assert!(!lowered.contains("delete from access_record"));
        }
        assert_eq!(store.access_record().unwrap().len(), 8);
    }

    #[test]
    fn the_access_record_moves_neither_the_revision_nor_the_rolling_backup() {
        let sandbox = Sandbox::new("access-revision");
        let store = sandbox.store();
        probe(&store, &Actor::Human, "first");
        probe(&store, &Actor::Human, "second");
        let revision = store.revision().unwrap();
        let backup_before = fs::read(sandbox.backup()).ok();
        let backup_modified = fs::metadata(sandbox.backup())
            .and_then(|m| m.modified())
            .ok();

        store.member_add("alice", Role::Admin, NOW).unwrap();
        store.member_add("bob", Role::Member, NOW).unwrap();
        store.member_set_role("bob", Role::Admin, NOW).unwrap();
        store.member_token_issue("bob", "laptop", NOW).unwrap();
        store.member_signin_key_issue("bob", NOW).unwrap();
        store
            .access_record_export(&AccessActor::owner(), "board", NOW)
            .unwrap();
        store.member_remove("bob", NOW).unwrap();
        assert!(store.access_record().unwrap().len() >= 7);

        assert_eq!(store.revision().unwrap(), revision, "no revision bump");
        assert_eq!(
            fs::read(sandbox.backup()).ok(),
            backup_before,
            "no backup roll"
        );
        assert_eq!(
            fs::metadata(sandbox.backup())
                .and_then(|m| m.modified())
                .ok(),
            backup_modified
        );
    }

    #[test]
    fn the_access_record_migration_adds_the_table_and_bumps_the_version() {
        let sandbox = Sandbox::new("access-migration");
        fs::create_dir_all(sandbox.home_path()).unwrap();
        let connection = Connection::open(sandbox.database()).unwrap();
        migrate_with(&connection, &MIGRATIONS[..14], &Actor::Human).expect("v14 applies");
        assert!(!has_table(&connection, "access_record").unwrap());
        drop(connection);

        probe(&sandbox.store(), &Actor::Human, "migrates");

        let connection = sandbox.connection();
        assert_eq!(ACCESS_RECORD_VERSION, 15);
        assert_eq!(version(&connection).unwrap(), Some(CURRENT_VERSION));
        assert_eq!(CURRENT_VERSION, 15);
        assert!(has_table(&connection, "access_record").unwrap());
        assert_eq!(
            count(&sandbox, "SELECT COUNT(*) FROM access_record"),
            0,
            "the table starts empty"
        );
        assert!(
            sandbox.backup().exists(),
            "a migrating write rolls the backup first"
        );
    }

    #[test]
    fn the_access_record_is_classified_carried_for_the_archive() {
        assert!(CARRIED_TABLES.contains(&"access_record"));
        assert!(!crate::store::export::EMPTIED_TABLES.contains(&"access_record"));
        let sandbox = Sandbox::new("access-carried");
        let store = sandbox.store();
        probe(&store, &Actor::Human, "first");
        store.member_add("alice", Role::Admin, NOW).unwrap();
        store
            .access_record_export(&AccessActor::owner(), "board", NOW)
            .unwrap();

        let copy = store.export_copy().expect("the copy is taken");
        let connection =
            Connection::open_with_flags(copy.path(), OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let events: Vec<String> = connection
            .prepare("SELECT event FROM access_record ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(
            events,
            ["member-added", "export"],
            "who exported and when travels with a move"
        );
    }
}
