//! The bikeshed's locked-write mechanics (`LockedDirectoryTests`): the sidecar
//! `flock` with its bounded retry and in-process queue, the atomic temp +
//! rename commit, the directory fsync, and the orphan sweep. Moved with
//! `bikeshed/locked.rs` (CUT-9); the store keeps no sidecar of its own.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use crate::store::StoreError;

static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

/// A throwaway directory the storage tests lock and write in.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let serial = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-bikeshed-locked-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the sandbox is creatable");
        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
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

fn storage(sandbox: &Sandbox) -> crate::bikeshed::locked::LockedDirectory {
    fs::create_dir_all(sandbox.root()).expect("the root is creatable");
    crate::bikeshed::locked::LockedDirectory::for_bikeshed(
        sandbox.root().to_path_buf(),
        sandbox.root().join("bikeshed.lock"),
    )
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
        .filter(|name| name.starts_with(".note-"))
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
    let stale = sandbox.root().join(".note-stale.tmp");
    fs::write(&stale, b"stale").unwrap();
    aged(&stale, 120);
    let fresh = sandbox.root().join(".note-fresh.tmp");
    fs::write(&fresh, b"fresh").unwrap();

    let _guard = storage.lock().expect("the lock runs");

    assert!(!stale.exists(), "the stale orphan was not removed");
    assert!(fresh.exists(), "the fresh temp was wrongly removed");
}

#[test]
fn the_orphan_sweep_walks_subdirectories() {
    let sandbox = Sandbox::new("orphans-nested");
    let storage = storage(&sandbox);
    let nested = sandbox.root().join("bikeshed").join(".note-old.tmp");
    fs::create_dir_all(nested.parent().unwrap()).unwrap();
    fs::write(&nested, b"stale").unwrap();
    aged(&nested, 120);

    let _guard = storage.lock().expect("the lock runs");

    assert!(
        !nested.exists(),
        "the vault's sweep reaches the temps beside every note"
    );
}

#[test]
fn lock_holds_the_exclusive_lock_for_the_guard() {
    let sandbox = Sandbox::new("excludes");
    let storage = storage(&sandbox);
    let lock = sandbox.root().join("bikeshed.lock");

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
    let lock = sandbox.root().join("bikeshed.lock");
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
        .recv_timeout(crate::bikeshed::locked::LOCK_TIMEOUT * 3)
        .expect("a nested acquisition answers at the window, it does not hang");
    assert!(
        locked,
        "a nested acquisition answers Locked, not Ok: it took {elapsed:?}"
    );
    assert!(
        elapsed >= crate::bikeshed::locked::LOCK_TIMEOUT,
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
    let plain = root.join("bikeshed.lock");
    let dotted = root.join(".").join("bikeshed.lock");
    assert_eq!(
        crate::bikeshed::locked::queue_key(&plain).expect("the plain spelling canonicalizes"),
        crate::bikeshed::locked::queue_key(&dotted).expect("the dotted spelling canonicalizes"),
        "two spellings of one lock file must key one queue"
    );

    // And they exclude each other: the dotted spelling waits out a holder
    // on the plain one instead of taking a second lock.
    let holder = crate::bikeshed::locked::LockedDirectory::new(plain, ".note-", root.to_path_buf());
    let waiter =
        crate::bikeshed::locked::LockedDirectory::new(dotted, ".note-", root.to_path_buf());
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
    let key = crate::bikeshed::locked::queue_key(&sandbox.root().join("bikeshed.lock"))
        .expect("the sandbox root canonicalizes");

    let held = storage.lock().expect("the holder takes the lock");
    // The holder's own ticket is taken; every later arrival adds one.
    let holder_ticket = crate::bikeshed::locked::queued(&key);
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
        let deadline = Instant::now() + crate::bikeshed::locked::LOCK_TIMEOUT;
        let wanted = holder_ticket + waiters.len() as u64;
        while crate::bikeshed::locked::queued(&key) < wanted {
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
                .recv_timeout(crate::bikeshed::locked::LOCK_TIMEOUT * 3)
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
    let missing = sandbox.root().join("missing").join("bikeshed.lock");

    let outcome = crate::bikeshed::locked::queue_key(&missing);

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
    let temp = sandbox.root().join(".note-ordered.tmp");
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
    let temp = sandbox.root().join(".note-failed.tmp");
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
    let temp = sandbox.root().join(".note-write-only.tmp");
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
    let outcome = crate::store::fsync_directory(&not_a_directory.join("sub"));

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
    crate::store::fsync_directory(sandbox.root()).expect("a real directory syncs");
}
