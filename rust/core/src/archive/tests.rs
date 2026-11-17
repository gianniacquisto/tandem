//! The archive's suite at its public seam: `archive::export` and
//! `archive::import` over temp homes (CUT-35).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use flate2::Compression;
use flate2::write::GzEncoder;
use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};

use super::*;
use crate::store::{NewComment, NewProject, NewTicket};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-archive-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the sandbox is creatable");
        Self { root }
    }

    fn home(&self, name: &str) -> Home {
        Home::resolve(Some(&self.root.join(name)), None, &self.root.join("user"))
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const LATER: &str = "2026-11-16T00:00:00Z";

/// A home with `tickets` tickets, each with a comment, and some notes.
fn seed(home: &Home, tickets: usize) {
    let store = Store::new(home.clone());
    store
        .write(&Actor::Human, |write| {
            for index in 0..tickets {
                let id = format!("00000000-0000-4000-8000-{index:012}");
                write.insert_ticket(NewTicket {
                    id: &id,
                    title: &format!("ticket {index}"),
                    description: Some("body"),
                    status: "ready",
                    archived: false,
                    label: None,
                    number: None,
                    origin: "human",
                    assignee: None,
                    link: None,
                    position: index as i64,
                    created_at: LATER,
                    completed_at: None,
                })?;
                write.insert_comment(NewComment {
                    ticket_id: &id,
                    at: LATER,
                    body: &format!("comment {index}"),
                })?;
            }
            Ok(())
        })
        .expect("the seed lands");
    // A second write, so the rolling backup exists beside the store.
    store
        .write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "p",
                position: 0,
            })
        })
        .expect("the second write lands");
    let notes = home.directory().join("bikeshed");
    fs::create_dir_all(notes.join("projects/p")).expect("the notes folder is creatable");
    fs::write(notes.join("Index.md"), "# Index\n[[projects/p/p]]\n").expect("a note writes");
    fs::write(notes.join("projects/p/p.md"), "# p\nbody\n").expect("a note writes");
}

fn export_bytes(home: &Home) -> Vec<u8> {
    let mut out = Vec::new();
    export(home, &mut out).expect("the export succeeds");
    out
}

/// `(name, bytes)` of every entry, in archive order.
fn entries_of(archive: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    tar.entries()
        .expect("the archive reads")
        .map(|entry| {
            let mut entry = entry.expect("an entry reads");
            let name = entry.path().expect("a name").to_string_lossy().to_string();
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .expect("an entry's bytes read");
            (name, bytes)
        })
        .collect()
}

fn count(home: &Home, table: &str) -> i64 {
    let connection = Connection::open_with_flags(
        home.directory().join("tandem.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("the store opens");
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("the count reads")
}

fn sha(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes).as_slice())
}

/// Every name directly or deeply under `root`, sorted, for before/after.
fn listing(root: &Path) -> Vec<String> {
    fn walk(path: &Path, prefix: &str, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
            out.push(name.clone());
            if entry.path().is_dir() {
                walk(&entry.path(), &name, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out.sort();
    out
}

/// One hand-built entry.
struct Raw {
    name: Vec<u8>,
    kind: tar::EntryType,
    data: Vec<u8>,
    link: Option<&'static str>,
}

impl Raw {
    fn file(name: &str, data: &[u8]) -> Self {
        Self {
            name: name.as_bytes().to_vec(),
            kind: tar::EntryType::Regular,
            data: data.to_vec(),
            link: None,
        }
    }

    fn of_kind(name: &str, kind: tar::EntryType, link: Option<&'static str>) -> Self {
        Self {
            name: name.as_bytes().to_vec(),
            kind,
            data: Vec::new(),
            link,
        }
    }
}

/// How the hand-built archive's manifest is made.
enum Manifested {
    /// Honest: lists every regular entry once, last.
    Honest,
    /// No manifest at all.
    Absent,
    /// This JSON text, as is.
    Text(String),
}

fn build(entries: &[Raw], manifest: Manifested) -> Vec<u8> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    let mut listed: Vec<ManifestEntry> = Vec::new();
    for entry in entries {
        let mut header = tar::Header::new_gnu();
        {
            let old = header.as_old_mut();
            old.name[..entry.name.len()].copy_from_slice(&entry.name);
        }
        header.set_entry_type(entry.kind);
        header.set_size(entry.data.len() as u64);
        header.set_mode(0o644);
        if let Some(link) = entry.link {
            header.set_link_name(link).expect("a link name");
        }
        header.set_cksum();
        builder
            .append(&header, entry.data.as_slice())
            .expect("an entry appends");
        if entry.kind == tar::EntryType::Regular
            && !listed.iter().any(|l| l.path.as_bytes() == entry.name)
        {
            listed.push(ManifestEntry {
                path: String::from_utf8_lossy(&entry.name).to_string(),
                size: entry.data.len() as u64,
                sha256: sha(&entry.data),
            });
        }
    }
    let text = match manifest {
        Manifested::Absent => None,
        Manifested::Text(text) => Some(text),
        Manifested::Honest => Some(
            serde_json::to_string(&Manifest {
                format: FORMAT_VERSION,
                schema_version: store::CURRENT_VERSION,
                tandem_version: "test".into(),
                created_at: LATER.into(),
                entries: listed,
            })
            .expect("a manifest serialises"),
        ),
    };
    if let Some(text) = text {
        let mut header = header_for(text.len() as u64, 0);
        builder
            .append_data(&mut header, MANIFEST_ENTRY, text.as_bytes())
            .expect("the manifest appends");
    }
    builder
        .into_inner()
        .expect("finish")
        .finish()
        .expect("finish")
}

/// The store's bytes from a real export of a seeded home.
fn real_store(sandbox: &Sandbox) -> Vec<u8> {
    let source = sandbox.home(&format!(
        "real-source-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    seed(&source, 2);
    entries_of(&export_bytes(&source))
        .into_iter()
        .find(|(name, _)| name == STORE_ENTRY)
        .expect("the archive holds the store")
        .1
}

/// A store whose file has been altered by `edit` (SQL), as archive bytes.
fn altered_store(sandbox: &Sandbox, sql: &str) -> Vec<u8> {
    let bytes = real_store(sandbox);
    let path = sandbox.root.join(format!(
        "altered-{}.db",
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, bytes).expect("the copy writes");
    {
        let connection = Connection::open(&path).expect("the copy opens");
        connection.execute_batch(sql).expect("the edit applies");
    }
    fs::read(path).expect("the copy reads")
}

fn is_refused<T: std::fmt::Debug>(result: Result<T, ArchiveError>) -> ArchiveError {
    result.expect_err("the import is refused")
}

fn principal(access: Access) -> Principal {
    Principal {
        access,
        actor: Actor::Human,
    }
}

// MARK: - 1 to 5: export

#[test]
fn an_exported_home_imports_with_the_same_tickets_comments_and_notes() {
    let sandbox = Sandbox::new("roundtrip");
    let source = sandbox.home("source");
    seed(&source, 3);
    let archive = export_bytes(&source);

    let target = sandbox.home("target");
    let summary = import(&target, archive.as_slice(), None).expect("the import lands");
    assert_eq!(summary.tickets, 3);
    assert_eq!(summary.notes, 2);
    assert_eq!(summary.schema_version, store::CURRENT_VERSION);
    assert_eq!(count(&target, "tickets"), count(&source, "tickets"));
    assert_eq!(count(&target, "comments"), 3);
    assert_eq!(
        fs::read_to_string(target.directory().join("bikeshed/projects/p/p.md")).unwrap(),
        "# p\nbody\n"
    );
    assert_eq!(
        fs::read(target.directory().join("bikeshed/Index.md")).unwrap(),
        fs::read(source.directory().join("bikeshed/Index.md")).unwrap()
    );
    // The imported store is a working store: it takes a write.
    Store::new(target.clone())
        .write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "after",
                position: 1,
            })
        })
        .expect("the imported store takes a write");
}

#[test]
fn the_archive_holds_the_store_the_notes_and_the_manifest_last_and_nothing_else() {
    let sandbox = Sandbox::new("shape");
    let source = sandbox.home("source");
    seed(&source, 1);
    // Decoys that must never travel.
    fs::write(source.directory().join("bikeshed.lock"), "").unwrap();
    fs::write(source.directory().join("tandem.lock"), "").unwrap();
    fs::write(source.directory().join(".tandem-old.tmp"), "x").unwrap();
    assert!(source.directory().join("tandem.db.bak").exists());

    let names: Vec<String> = entries_of(&export_bytes(&source))
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        names,
        [
            "tandem.db",
            "bikeshed/Index.md",
            "bikeshed/projects/p/p.md",
            "manifest.json"
        ]
    );
}

#[test]
fn the_manifest_names_the_format_the_schema_and_each_entrys_size_and_digest() {
    let sandbox = Sandbox::new("manifest");
    let source = sandbox.home("source");
    seed(&source, 1);
    let entries = entries_of(&export_bytes(&source));
    let manifest: Manifest =
        serde_json::from_slice(&entries.last().unwrap().1).expect("the manifest parses");
    assert_eq!(manifest.format, 1);
    assert_eq!(manifest.schema_version, store::CURRENT_VERSION);
    assert_eq!(manifest.tandem_version, crate::release_version());
    assert!(decode_time(&manifest.created_at));
    assert_eq!(manifest.entries.len(), entries.len() - 1);
    for (listed, (name, bytes)) in manifest.entries.iter().zip(&entries) {
        assert_eq!(&listed.path, name);
        assert_eq!(listed.size, bytes.len() as u64);
        assert_eq!(listed.sha256, sha(bytes));
    }
}

fn decode_time(text: &str) -> bool {
    store::decode(text).is_some()
}

#[test]
fn an_export_beside_an_active_writer_yields_a_store_that_opens() {
    let sandbox = Sandbox::new("writer");
    let source = sandbox.home("source");
    seed(&source, 2);
    let writer_home = source.clone();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop.clone();
    let writer = std::thread::spawn(move || {
        let store = Store::new(writer_home);
        let mut index = 0;
        while !flag.load(Ordering::Relaxed) {
            let name = format!("busy-{index}");
            let _ = store.write(&Actor::Human, |write| {
                write.insert_project(NewProject {
                    name: &name,
                    position: 100 + index,
                })
            });
            index += 1;
        }
    });
    for round in 0..5 {
        let archive = export_bytes(&source);
        let target = sandbox.home(&format!("target-{round}"));
        import(&target, archive.as_slice(), None).expect("each export imports");
        assert!(count(&target, "tickets") >= 2);
    }
    stop.store(true, Ordering::Relaxed);
    writer.join().expect("the writer ends");
}

#[test]
fn export_skips_symlinks_and_temp_files_under_the_notes_root() {
    let sandbox = Sandbox::new("skips");
    let source = sandbox.home("source");
    seed(&source, 1);
    let notes = source.directory().join("bikeshed");
    fs::write(notes.join(".note-abc.tmp"), "half").unwrap();
    fs::write(notes.join("projects/.tandem-xyz.tmp"), "half").unwrap();
    #[cfg(unix)]
    {
        let outside = sandbox.root.join("outside.md");
        fs::write(&outside, "secret outside the vault").unwrap();
        std::os::unix::fs::symlink(&outside, notes.join("link.md")).unwrap();
        std::os::unix::fs::symlink(sandbox.root.join("user"), notes.join("linked-dir")).unwrap();
    }
    let names: Vec<String> = entries_of(&export_bytes(&source))
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        names,
        [
            "tandem.db",
            "bikeshed/Index.md",
            "bikeshed/projects/p/p.md",
            "manifest.json"
        ],
        "{names:?}"
    );
}

// MARK: - 6 to 12: import refusals

#[test]
fn import_refuses_a_home_with_a_store_notes_a_backup_or_a_journal() {
    let sandbox = Sandbox::new("nonempty");
    let archive = export_bytes(&{
        let source = sandbox.home("source");
        seed(&source, 1);
        source
    });
    type Case<'a> = (&'a str, &'a [(&'a str, &'a str)], bool);
    let cases: [Case; 9] = [
        ("a store", &[("tandem.db", "x")], false),
        ("a backup", &[("tandem.db.bak", "x")], false),
        ("a wal", &[("tandem.db-wal", "x")], false),
        ("a journal", &[("tandem.db-journal", "x")], false),
        ("a note", &[("bikeshed/a.md", "x")], false),
        ("a nested note", &[("bikeshed/p/a.md", "x")], false),
        ("a lock file alone", &[("bikeshed.lock", "")], true),
        ("a tandem lock alone", &[("tandem.lock", "")], true),
        ("an empty notes folder", &[("bikeshed/.keep-dir", "")], true),
    ];
    for (index, (name, files, allowed)) in cases.iter().enumerate() {
        let target = sandbox.home(&format!("target-{index}"));
        for (file, text) in *files {
            let path = target.directory().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            if *file == "bikeshed/.keep-dir" {
                fs::create_dir_all(target.directory().join("bikeshed")).unwrap();
            } else {
                fs::write(path, text).unwrap();
            }
        }
        let result = import(&target, archive.as_slice(), None);
        if *allowed {
            result.unwrap_or_else(|error| panic!("{name} must pass: {error}"));
        } else {
            match is_refused(result) {
                ArchiveError::NotEmpty { found, .. } => {
                    assert!(!found.is_empty(), "{name}");
                    let message = ArchiveError::NotEmpty {
                        home: target.directory().to_path_buf(),
                        found,
                    }
                    .to_string();
                    assert!(message.contains("new, empty directory"), "{message}");
                }
                other => panic!("{name}: {other:?}"),
            }
        }
    }
}

#[test]
fn import_refuses_a_newer_schema_read_from_the_store_itself() {
    let sandbox = Sandbox::new("newer");
    // The manifest says the current schema; the file says v99.
    let db = altered_store(
        &sandbox,
        "INSERT INTO schema_migrations (version, applied_at, author) VALUES (99, 'x', 'human')",
    );
    let archive = build(&[Raw::file(STORE_ENTRY, &db)], Manifested::Honest);
    let target = sandbox.home("target");
    match is_refused(import(&target, archive.as_slice(), None)) {
        ArchiveError::Newer { version } => assert_eq!(version, 99),
        other => panic!("{other:?}"),
    }
    assert!(!target.directory().exists());
}

#[test]
fn import_refuses_a_cut_archive_a_missing_manifest_and_a_digest_or_size_mismatch() {
    let sandbox = Sandbox::new("damaged");
    let source = sandbox.home("source");
    seed(&source, 2);
    let whole = export_bytes(&source);
    let db = entries_of(&whole)[0].1.clone();
    let target = sandbox.home("target");

    // Cut in half: the gzip stream ends early.
    let cut = &whole[..whole.len() / 2];
    assert!(matches!(
        is_refused(import(&target, cut, None)),
        ArchiveError::Damaged(_)
    ));

    // A whole tar with no manifest, as a cut at an entry boundary leaves it.
    let no_manifest = build(&[Raw::file(STORE_ENTRY, &db)], Manifested::Absent);
    match is_refused(import(&target, no_manifest.as_slice(), None)) {
        ArchiveError::Damaged(why) => assert!(why.contains("manifest"), "{why}"),
        other => panic!("{other:?}"),
    }

    // A digest that does not match.
    let wrong_digest = Manifested::Text(format!(
        r#"{{"format":1,"schema_version":12,"tandem_version":"t","created_at":"{LATER}","entries":[{{"path":"tandem.db","size":{},"sha256":"{}"}}]}}"#,
        db.len(),
        "0".repeat(64)
    ));
    match is_refused(import(
        &target,
        build(&[Raw::file(STORE_ENTRY, &db)], wrong_digest).as_slice(),
        None,
    )) {
        ArchiveError::Damaged(why) => assert!(why.contains("digest"), "{why}"),
        other => panic!("{other:?}"),
    }

    // A size that does not match.
    let wrong_size = Manifested::Text(format!(
        r#"{{"format":1,"schema_version":12,"tandem_version":"t","created_at":"{LATER}","entries":[{{"path":"tandem.db","size":1,"sha256":"{}"}}]}}"#,
        sha(&db)
    ));
    match is_refused(import(
        &target,
        build(&[Raw::file(STORE_ENTRY, &db)], wrong_size).as_slice(),
        None,
    )) {
        ArchiveError::Damaged(why) => assert!(why.contains("bytes"), "{why}"),
        other => panic!("{other:?}"),
    }

    // A listed file the archive does not hold.
    let missing = Manifested::Text(format!(
        r#"{{"format":1,"schema_version":12,"tandem_version":"t","created_at":"{LATER}","entries":[{{"path":"tandem.db","size":{},"sha256":"{}"}},{{"path":"bikeshed/gone.md","size":1,"sha256":"{}"}}]}}"#,
        db.len(),
        sha(&db),
        "0".repeat(64)
    ));
    assert!(matches!(
        is_refused(import(
            &target,
            build(&[Raw::file(STORE_ENTRY, &db)], missing).as_slice(),
            None
        )),
        ArchiveError::Damaged(_)
    ));

    // A format this build does not know.
    let future = Manifested::Text(format!(
        r#"{{"format":2,"schema_version":12,"tandem_version":"t","created_at":"{LATER}","entries":[]}}"#
    ));
    assert!(matches!(
        is_refused(import(
            &target,
            build(&[Raw::file(STORE_ENTRY, &db)], future).as_slice(),
            None
        )),
        ArchiveError::Format { found: 2 }
    ));
    assert!(!target.directory().exists());
}

#[test]
fn import_refuses_hostile_entries() {
    let sandbox = Sandbox::new("hostile");
    let db = real_store(&sandbox);
    let store_entry = || Raw::file(STORE_ENTRY, &db);
    let cases: Vec<(&str, Vec<Raw>)> = vec![
        (
            "absolute",
            vec![store_entry(), Raw::file("/etc/passwd", b"x")],
        ),
        (
            "dot-dot",
            vec![store_entry(), Raw::file("bikeshed/../escape.md", b"x")],
        ),
        ("leading dot-dot", vec![Raw::file("../escape", b"x")]),
        (
            "backslash",
            vec![store_entry(), Raw::file("bikeshed\\evil.md", b"x")],
        ),
        (
            "empty component",
            vec![store_entry(), Raw::file("bikeshed//a.md", b"x")],
        ),
        (
            "symlink",
            vec![
                store_entry(),
                Raw::of_kind(
                    "bikeshed/l.md",
                    tar::EntryType::Symlink,
                    Some("/etc/passwd"),
                ),
            ],
        ),
        (
            "hard link",
            vec![
                store_entry(),
                Raw::of_kind("bikeshed/h.md", tar::EntryType::Link, Some("tandem.db")),
            ],
        ),
        (
            "device",
            vec![
                store_entry(),
                Raw::of_kind("bikeshed/dev", tar::EntryType::Char, None),
            ],
        ),
        (
            "fifo",
            vec![
                store_entry(),
                Raw::of_kind("bikeshed/fifo", tar::EntryType::Fifo, None),
            ],
        ),
        (
            "outside the three roots",
            vec![store_entry(), Raw::file("other/x.md", b"x")],
        ),
        (
            "a wal beside the store",
            vec![store_entry(), Raw::file("tandem.db-wal", b"x")],
        ),
        (
            "duplicate",
            vec![
                store_entry(),
                Raw::file("bikeshed/a.md", b"one"),
                Raw::file("bikeshed/a.md", b"two"),
            ],
        ),
        (
            "unlisted",
            vec![store_entry(), Raw::file("bikeshed/unlisted.md", b"x")],
        ),
    ];
    for (name, entries) in cases {
        let target = sandbox.home("target");
        // The unlisted case needs a manifest that does not list its entry.
        let manifest = if name == "unlisted" {
            Manifested::Text(format!(
                r#"{{"format":1,"schema_version":12,"tandem_version":"t","created_at":"{LATER}","entries":[{{"path":"tandem.db","size":{},"sha256":"{}"}}]}}"#,
                db.len(),
                sha(&db)
            ))
        } else {
            Manifested::Honest
        };
        let archive = build(&entries, manifest);
        match is_refused(import(&target, archive.as_slice(), None)) {
            ArchiveError::Hostile { entry, reason } => {
                assert!(!entry.is_empty() && !reason.is_empty(), "{name}");
            }
            other => panic!("{name}: {other:?}"),
        }
        assert!(!target.directory().exists(), "{name} left a home behind");
    }
}

#[test]
fn import_stops_at_the_byte_ceiling() {
    let sandbox = Sandbox::new("ceiling");
    let source = sandbox.home("source");
    seed(&source, 1);
    let archive = export_bytes(&source);
    let target = sandbox.home("target");
    match is_refused(import(&target, archive.as_slice(), Some(100))) {
        ArchiveError::TooLarge { limit } => assert_eq!(limit, 100),
        other => panic!("{other:?}"),
    }
    assert!(!target.directory().exists());
    import(&target, archive.as_slice(), Some(10_000_000)).expect("a roomy ceiling passes");
}

#[test]
fn import_refuses_a_store_with_a_foreign_trigger_or_a_filled_emptied_table() {
    let sandbox = Sandbox::new("foreign");
    let target = sandbox.home("target");

    let trigger = altered_store(
        &sandbox,
        "CREATE TRIGGER evil AFTER INSERT ON tickets BEGIN DELETE FROM comments; END",
    );
    assert!(matches!(
        is_refused(import(
            &target,
            build(&[Raw::file(STORE_ENTRY, &trigger)], Manifested::Honest).as_slice(),
            None
        )),
        ArchiveError::Refused(_)
    ));

    let view = altered_store(&sandbox, "CREATE VIEW sneaky AS SELECT * FROM tickets");
    assert!(matches!(
        is_refused(import(
            &target,
            build(&[Raw::file(STORE_ENTRY, &view)], Manifested::Honest).as_slice(),
            None
        )),
        ArchiveError::Refused(_)
    ));

    let rogue = altered_store(&sandbox, "CREATE TABLE rogue (x TEXT)");
    assert!(matches!(
        is_refused(import(
            &target,
            build(&[Raw::file(STORE_ENTRY, &rogue)], Manifested::Honest).as_slice(),
            None
        )),
        ArchiveError::Refused(_)
    ));

    let filled = altered_store(
        &sandbox,
        "CREATE TABLE secret_probe (token TEXT); INSERT INTO secret_probe VALUES ('s')",
    );
    let path = sandbox.root.join("filled.db");
    fs::write(&path, filled).unwrap();
    let refusal = store::inspect_with(
        &path,
        &[
            "tickets",
            "ticket_dependencies",
            "comments",
            "labels",
            "projects",
            "store_meta",
            "schema_migrations",
        ],
        &["secret_probe"],
    )
    .expect_err("a filled emptied table is refused");
    assert!(
        matches!(&refusal, Refusal::Unexpected(why) if why.contains("secret_probe")),
        "{refusal:?}"
    );
    assert!(!target.directory().exists());
}

#[test]
fn a_refused_import_leaves_the_home_as_it_was_and_no_temp_directory() {
    let sandbox = Sandbox::new("untouched");
    let source = sandbox.home("source");
    seed(&source, 1);
    let good = export_bytes(&source);
    let db = entries_of(&good)[0].1.clone();

    let target = sandbox.home("target");
    fs::create_dir_all(target.directory()).unwrap();
    fs::write(target.directory().join("bikeshed.lock"), "").unwrap();
    let before = listing(target.directory());

    let broken = build(
        &[
            Raw::file(STORE_ENTRY, &db),
            Raw::file("bikeshed/a.md", b"x"),
        ],
        Manifested::Absent,
    );
    for archive in [
        &good[..good.len() / 3],
        broken.as_slice(),
        b"not gzip at all",
    ] {
        let _ = is_refused(import(&target, archive, None));
        assert_eq!(listing(target.directory()), before);
    }
    let _ = is_refused(import(&target, good.as_slice(), Some(1)));
    assert_eq!(listing(target.directory()), before);
    assert!(
        !listing(target.directory())
            .iter()
            .any(|name| name.contains(".tandem-import-")),
    );
}

// MARK: - 13 and 14: older schema, privacy

#[test]
fn an_older_schema_archive_imports_and_migrates_on_the_first_write() {
    let sandbox = Sandbox::new("older");
    let source = sandbox.home("source");
    fs::create_dir_all(source.directory()).unwrap();
    {
        let connection = Connection::open(source.directory().join("tandem.db")).unwrap();
        store::schema::migrate_with(&connection, &store::schema::MIGRATIONS[..11], &Actor::Human)
            .expect("a v11 store builds");
    }
    let archive = export_bytes(&source);
    let target = sandbox.home("target");
    let summary = import(&target, archive.as_slice(), None).expect("an older store imports");
    assert_eq!(summary.schema_version, 11);
    let store = Store::new(target.clone());
    assert_eq!(store.schema_version().unwrap(), Some(11));
    store
        .write(&Actor::Human, |write| {
            write.insert_project(NewProject {
                name: "first",
                position: 0,
            })
        })
        .expect("the first write migrates");
    assert_eq!(
        store.schema_version().unwrap(),
        Some(store::CURRENT_VERSION)
    );
    assert!(
        target.directory().join("tandem.db.bak").exists(),
        "the migrating write rolled the backup first"
    );
}

#[cfg(unix)]
#[test]
fn unpacked_files_and_the_export_file_are_private_to_the_owner() {
    use std::os::unix::fs::PermissionsExt;
    let sandbox = Sandbox::new("private");
    let source = sandbox.home("source");
    seed(&source, 1);
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;

    // The export's copy of the store.
    let copy = Store::new(source.clone()).export_copy().expect("a copy");
    assert_eq!(mode(copy.path()), 0o600);
    drop(copy);

    // The unpacked files arrive private, whatever the archive's modes say.
    let archive = export_bytes(&source);
    let target = sandbox.home("target");
    import(&target, archive.as_slice(), None).expect("the import lands");
    assert_eq!(mode(&target.directory().join("tandem.db")), 0o600);
    assert_eq!(mode(&target.directory().join("bikeshed/Index.md")), 0o600);
    assert_eq!(mode(&target.directory().join("bikeshed/projects")), 0o700);
}

// MARK: - 15 and 16: the permission and the record

#[test]
fn the_export_permission_allows_every_principal_that_exists_today_and_fails_closed() {
    for access in [
        Access::Local,
        Access::Loopback,
        Access::Unauthenticated,
        Access::Token,
    ] {
        may_export(&principal(access)).unwrap_or_else(|error| panic!("{access:?}: {error}"));
    }
    assert!(matches!(
        may_export(&principal(Access::Unknown)),
        Err(ArchiveError::NotAllowed)
    ));
    // The one entry a door calls: the check, then the record.
    assert!(start_export(&principal(Access::Unknown), Door::Board).is_err());
    assert!(start_export(&principal(Access::Token), Door::Board).is_ok());
}

#[test]
fn the_export_record_names_the_actor_the_time_and_the_door_and_nothing_else() {
    let line = record_export(&Actor::Agent("builder".into()), Door::Cli);
    let fields: Vec<&str> = line.split(' ').collect();
    assert_eq!(fields.len(), 4, "{line}");
    assert_eq!(fields[0], "export");
    assert_eq!(fields[1], "actor=builder");
    assert_eq!(fields[2], "door=cli");
    let at = fields[3].strip_prefix("at=").expect("a time field");
    assert!(decode_time(at), "{at}");
    assert!(record_export(&Actor::Human, Door::Board).contains("door=board"));
    // An actor cannot smuggle a second field or a path into the line.
    let odd = record_export(&Actor::Agent("a b/c".into()), Door::Cli);
    assert_eq!(odd.split(' ').count(), 4, "{odd}");
    assert!(!odd.contains('/'), "{odd}");
}
