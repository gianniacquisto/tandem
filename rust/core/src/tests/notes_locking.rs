//! Ported from `NotesLockingTests`: two writers on threads serialize under the
//! sidecar `flock` with no lost or torn notes; a holder past the bounded retry
//! window yields `NotesLocked`; a lock-free reader never observes a partial
//! note from an external writer; stale temps are swept and fresh ones are kept.

use std::fs::{self, File};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::domain::error::DomainError;
use crate::notes::Notes;

use super::notes_support::Vault;

/// A loaded runner can spend more than the production 2 s window on one note
/// write (a Defender scan of the fsync, a slow VM), and the window bounds a
/// writer's wait for the holder ahead of it. The contention case configures its
/// own wider window; `LOCK_TIMEOUT` stays 2 s, and
/// `a_configured_lock_window_still_bites_after_a_deliberate_hold` proves a
/// configured window still bites.
const SLOW_RUNNER_WINDOW: Duration = Duration::from_secs(60);

#[test]
fn two_writers_on_threads_interleave_without_lost_or_torn_notes() {
    let vault = Vault::new("lock-contended");
    vault.stage("A.md", "");
    vault.stage("B.md", "");
    let notes = Arc::new(vault.notes().with_lock_timeout(SLOW_RUNNER_WINDOW));
    let errors = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::new();
    for tag in ["X", "Y"] {
        let notes = Arc::clone(&notes);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            for i in 0..20 {
                for path in ["A.md", "B.md"] {
                    if let Err(error) = notes.write(path, &format!("{tag}{i}"), None) {
                        errors.lock().unwrap().push(error.message());
                    }
                }
            }
        }));
    }
    for handle in handles {
        handle.join().expect("a writer thread finishes");
    }

    assert!(errors.lock().unwrap().is_empty(), "a writer hit an error");
    let final_a = vault.read("A.md");
    let final_b = vault.read("B.md");
    assert!(
        final_a == "X19" || final_a == "Y19",
        "torn or lost A.md: {final_a}"
    );
    assert!(
        final_b == "X19" || final_b == "Y19",
        "torn or lost B.md: {final_b}"
    );
}

#[test]
fn a_write_on_a_locked_notes_throws_locked_after_the_bounded_retry() {
    let vault = Vault::new("lock-locked");
    vault.stage("A.md", "seed");
    let notes = vault.notes();
    let lock_path = vault.lock_path();
    let (ready, acquired) = mpsc::channel();

    let holder = thread::spawn(move || {
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .expect("the lock file opens");
        file.lock().expect("the holder takes the lock");
        ready.send(()).expect("the holder signals");
        thread::sleep(Duration::from_millis(2_500));
    });
    acquired
        .recv_timeout(Duration::from_secs(15))
        .expect("the holder acquired the lock");

    assert_eq!(
        notes
            .write("A.md", "should fail while locked", None)
            .unwrap_err(),
        DomainError::NotesLocked
    );
    holder.join().expect("the holder releases");

    notes.write("A.md", "after release", None).unwrap();
    assert_eq!(vault.read("A.md"), "after release");
}

/// A vault with its own short window still answers `Locked` when a holder
/// outlasts it: the wider window the contention case configures is a setting,
/// never a disabled bound, and the production `LOCK_TIMEOUT` is untouched.
#[test]
fn a_configured_lock_window_still_bites_after_a_deliberate_hold() {
    let vault = Vault::new("lock-configured");
    vault.stage("A.md", "seed");
    let notes = vault.notes().with_lock_timeout(Duration::from_millis(200));
    let lock_path = vault.lock_path();
    let (ready, acquired) = mpsc::channel();

    let holder = thread::spawn(move || {
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .expect("the lock file opens");
        file.lock().expect("the holder takes the lock");
        ready.send(()).expect("the holder signals");
        thread::sleep(Duration::from_millis(600));
    });
    acquired
        .recv_timeout(Duration::from_secs(15))
        .expect("the holder acquired the lock");

    let started = Instant::now();
    assert_eq!(
        notes
            .write("A.md", "should fail while locked", None)
            .unwrap_err(),
        DomainError::NotesLocked
    );
    let elapsed = started.elapsed();
    holder.join().expect("the holder releases");

    assert!(
        elapsed >= Duration::from_millis(150),
        "the writer waited the configured window: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the writer did not wait far past the configured window: {elapsed:?}"
    );
    notes.write("A.md", "after release", None).unwrap();
    assert_eq!(vault.read("A.md"), "after release");
}

#[test]
fn a_lock_free_reader_never_observes_a_partial_note() {
    let vault = Vault::new("lock-reader");
    vault.stage("A.md", "x");
    let notes = Notes::new(vault.dir());
    let done = Arc::new(AtomicBool::new(false));
    let writer_done = Arc::clone(&done);

    let writer = thread::spawn(move || {
        let path = vault.dir().join("A.md");
        let temp = vault.dir().join(".external-write.tmp");
        for i in 1..=120 {
            let text = "x".repeat(i);
            fs::write(&temp, &text).expect("the temp is written");
            fs::rename(&temp, &path).expect("the rename is atomic");
        }
        writer_done.store(true, Ordering::SeqCst);
    });

    let mut reads = 0;
    let mut partial = None;
    while !done.load(Ordering::SeqCst) {
        reads += 1;
        if let Ok(result) = notes.read("A.md") {
            let text = result.value.text;
            let complete = !text.is_empty()
                && text.chars().all(|character| character == 'x')
                && text.len() <= 120;
            if !complete {
                partial = Some(text);
                break;
            }
        }
    }
    writer.join().expect("the external writer finishes");

    assert!(reads > 0, "the reader never ran");
    assert_eq!(partial, None, "the reader observed an incomplete note");
}

#[test]
fn stale_orphan_temps_are_removed_and_fresh_temps_are_kept() {
    let vault = Vault::new("lock-orphans");
    vault.stage("A.md", "seed");
    let notes = vault.notes();

    let orphan = "deep/.note-crash-orphan.tmp";
    vault.stage(orphan, "orphan");
    vault.set_modified(
        orphan,
        std::time::SystemTime::now() - Duration::from_secs(120),
    );
    let fresh = ".note-fresh.tmp";
    vault.stage(fresh, "fresh");

    notes.write("A.md", "trigger cleanup", None).unwrap();

    assert!(!vault.exists(orphan), "the stale orphan was not removed");
    assert!(vault.exists(fresh), "a fresh temp was wrongly removed");
}
