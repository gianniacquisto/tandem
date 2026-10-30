//! The storage mechanics a file-tree writer shares: a `flock` sidecar with
//! bounded retry, same-directory temp files with `fsync` + rename, and the
//! orphan sweep that clears a crash's leftovers under the lock.
//!
//! Ported from `LockedDirectory.swift` and its tests (`LockedDirectoryTests`),
//! behaviour for behaviour: the lock is never a suggestion, a write lands whole
//! or not at all, and a temp older than a minute is a crash's, not a writer's.
//! This module is the mechanism only. It owns no schema and no domain
//! vocabulary; the store translates its two failures into the store's words.
//!
//! The store takes no lock (ADR 0031): SQLite's `BEGIN IMMEDIATE` and the busy
//! window are its contention bound, and it borrows only the temp, the commit
//! and the directory sync for the rolling backup. The sidecar, the sweep and
//! [`LockedDirectory::lock`] are the bikeshed's, which is the file tree that
//! still needs one.
//!
//! One thing the Swift mechanism did not need and this one does: `flock` has no
//! queue, so threads of one process that want the same lock race `try_lock`
//! and the loser can lose every flip for the whole window. [`Queue`] gives this
//! process's bikeshed writers a turn order, so a waiter is served when the
//! holder releases instead of racing it, and the window bounds the wait rather
//! than the luck. A writer that cannot be served within the window, queued or
//! blocked by a foreign holder, answers `Locked` exactly as it did before the
//! queue.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{Result, StoreError};

/// How long a writer waits for the sidecar lock before answering "locked".
///
/// The window bounds the whole wait: this process's queue ([`Queue`]) and then
/// the `flock` retry. The queue is what keeps a waiter from spending it racing a
/// lock this process is about to release, so the window covers one turn per
/// writer ahead instead of a coin flip per release.
pub(crate) const LOCK_TIMEOUT: Duration = Duration::from_millis(2_000);

/// A temp older than this is a crash's leftover, never an in-flight write's.
pub(crate) const ORPHAN_MAX_AGE: Duration = Duration::from_secs(60);

/// The store's temp prefix: its own files only, so the sweep cannot touch the
/// bikeshed's.
pub(crate) const TEMP_PREFIX: &str = ".tandem-";

const TEMP_SUFFIX: &str = ".tmp";

/// The bikeshed's temp prefix: its notes are written beside the note, so the
/// sweep recurses into the tree.
pub(crate) const BIKESHED_TEMP_PREFIX: &str = ".note-";

/// The turn order of this process's writers, one queue per lock file.
///
/// `flock` has no queue: two threads that both spin on `try_lock` are a coin
/// flip per release, and the loser of every flip spends the whole window and
/// answers `Locked` while the lock stood free between the holder's writes
/// (`rust (windows-latest)` run 41099651828,
/// `a_concurrent_edit_is_not_reverted_by_another_writers_snapshot`). A ticket
/// queue removes the race: the holder closes its handle, which is what releases
/// the `flock`, *before* it serves the next ticket, so the waiter it wakes
/// finds the lock free.
///
/// The wait is bounded like the lock's: a ticket that is not served within
/// [`LOCK_TIMEOUT`] answers `Locked` and is skipped, so a body that takes the
/// same lock file again (a nested acquisition) answers at the window instead of
/// hanging. The bikeshed's write bodies never nest; this is the guard that
/// makes that constraint fail loudly if one ever does.
struct Queue {
    turn: Mutex<Turn>,
    served: Condvar,
}

struct Turn {
    /// The ticket the next arrival takes.
    next: u64,
    /// The ticket allowed to run.
    serving: u64,
}

impl Queue {
    fn new() -> Self {
        Self {
            turn: Mutex::new(Turn {
                next: 0,
                serving: 0,
            }),
            served: Condvar::new(),
        }
    }

    /// Take a ticket and wait until it is served, or answer `Locked` when
    /// `deadline` passes: a ticket that is never served (a nested acquisition, a
    /// holder that never releases) must not hang the process.
    fn take(&self, deadline: Instant) -> Result<u64> {
        let mut turn = lock(&self.turn);
        let ticket = turn.next;
        turn.next += 1;
        loop {
            if turn.serving == ticket {
                return Ok(ticket);
            }
            let now = Instant::now();
            if now >= deadline {
                // Leave without stranding the queue: tickets are served in
                // order, so a ticket that timed out is skipped and the writers
                // behind it are served when their turn comes.
                turn.serving = turn.serving.max(ticket + 1);
                drop(turn);
                self.served.notify_all();
                return Err(StoreError::Locked);
            }
            turn = self
                .served
                .wait_timeout(turn, deadline - now)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }

    /// Serve the ticket after `ticket`: writers queue in ticket order, so the
    /// ticket that released is the one that was being served. A ticket that
    /// timed out may have served later ones already, so this never moves the
    /// queue backwards.
    fn release(&self, ticket: u64) {
        let mut turn = lock(&self.turn);
        turn.serving = turn.serving.max(ticket + 1);
        drop(turn);
        self.served.notify_all();
    }
}

/// Every lock file this process has a queue for, keyed by [`queue_key`].
static QUEUES: OnceLock<Mutex<HashMap<PathBuf, Arc<Queue>>>> = OnceLock::new();

/// The key a lock file's queue is filed under: its directory canonicalized, so
/// two spellings of one lock file (a symlinked home, a `.` or `..` segment, a
/// different case on Windows) share one queue. The file itself may not exist
/// yet, so the directory is what canonicalizes.
///
/// A directory that cannot canonicalize is an error, never the path as
/// spelled: the spelled path is exactly the split this key exists to close
/// (two spellings of one home would each get a queue and race the `flock`
/// again), and every caller has just created the directory, so a failure here
/// is a filesystem problem to name, not a spelling variant to tolerate.
pub(crate) fn queue_key(lock_file: &Path) -> Result<PathBuf> {
    let name = lock_file.file_name().ok_or_else(|| {
        StoreError::Io(format!(
            "the lock file {} has no name to key its queue on",
            lock_file.display()
        ))
    })?;
    let directory = lock_file.parent().unwrap_or_else(|| Path::new("."));
    let directory = directory.canonicalize().map_err(|error| {
        StoreError::Io(format!(
            "could not canonicalize the directory {}: {error}",
            directory.display()
        ))
    })?;
    Ok(directory.join(name))
}

/// The queue for one lock file, created on first use.
fn queue_for(key: &Path) -> Arc<Queue> {
    let mut queues = lock(QUEUES.get_or_init(|| Mutex::new(HashMap::new())));
    queues
        .entry(key.to_path_buf())
        .or_insert_with(|| Arc::new(Queue::new()))
        .clone()
}

/// The number of tickets this process has taken for `key`: test-facing, so a
/// case can wait until a waiter has queued instead of assuming it by a sleep.
#[cfg(test)]
pub(crate) fn queued(key: &Path) -> u64 {
    let Some(queues) = QUEUES.get() else {
        return 0;
    };
    let queues = lock(queues);
    queues
        .get(key)
        .map(|queue| lock(&queue.turn).next)
        .unwrap_or(0)
}

/// Drop a lock file's queue once no writer of this process holds or waits on
/// it, so a process that writes to many homes keeps no entry per home.
fn forget(key: &Path, queue: &Arc<Queue>) {
    let Some(queues) = QUEUES.get() else {
        return;
    };
    let mut queues = lock(queues);
    // The registry's clone and this one, and `queue_for` cannot clone it under
    // this same lock: the entry is safe to remove.
    if Arc::strong_count(queue) == 2 && queues.get(key).is_some_and(|held| Arc::ptr_eq(held, queue))
    {
        queues.remove(key);
    }
}

/// A mutex whose poisoning cannot lose a ticket: no thread panics while it
/// holds this state, only between operations on it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One domain's locked directory: the sidecar lockfile, the temp naming and the
/// tree the orphan sweep walks.
#[derive(Debug, Clone)]
pub(crate) struct LockedDirectory {
    lock_file: PathBuf,
    temp_prefix: String,
    root: PathBuf,
    /// True for the bikeshed, whose temps sit beside each note in a nested
    /// tree; false for the store, one flat file per directory.
    recursive: bool,
}

impl LockedDirectory {
    pub(crate) fn new(lock_file: PathBuf, temp_prefix: impl Into<String>, root: PathBuf) -> Self {
        Self {
            lock_file,
            temp_prefix: temp_prefix.into(),
            root,
            recursive: false,
        }
    }

    /// The store's own directory: `<home>/tandem.lock` as the lock file and the
    /// home as its root. The store takes no lock (ADR 0031) and borrows this
    /// value for the rolling backup's commit and directory sync; the sidecar
    /// itself is the bikeshed's.
    pub(crate) fn for_store(home: &Path) -> Self {
        Self::new(home.join("tandem.lock"), TEMP_PREFIX, home.to_path_buf())
    }

    /// The bikeshed's sidecar: `<home>/bikeshed.lock` (outside the vault) and the
    /// vault as its root, swept recursively (ADR 0004).
    pub(crate) fn for_bikeshed(root: PathBuf, lock_file: PathBuf) -> Self {
        Self {
            lock_file,
            temp_prefix: BIKESHED_TEMP_PREFIX.to_string(),
            root,
            recursive: true,
        }
    }

    /// Acquire the lock and sweep, handing back the guard that holds it. A
    /// caller with its own error vocabulary (the bikeshed) translates the two
    /// failures itself and keeps the body's errors its own.
    pub(crate) fn lock(&self) -> Result<LockGuard> {
        let held = self.acquire()?;
        self.sweep_orphans();
        Ok(held)
    }

    /// Write `bytes` to `target` through a same-directory temp, then commit it.
    /// Missing parent folders are created, so a first write into a deeper path
    /// lands the same way a file's did (ADR 0001).
    pub(crate) fn atomic_write(&self, bytes: &[u8], target: &Path) -> Result<()> {
        let parent = target.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| {
            StoreError::Io(format!(
                "could not create the directory {}: {error}",
                parent.display()
            ))
        })?;
        let temp = self.temp_for(target);
        fs::write(&temp, bytes).map_err(|error| {
            StoreError::Io(format!(
                "could not write the temp at {}: {error}",
                temp.display()
            ))
        })?;
        self.commit(&temp, target, || Ok(()))
    }

    /// The temp file an atomic write of `target` stages in.
    pub(crate) fn temp_for(&self, target: &Path) -> PathBuf {
        sibling(target, &self.temp_prefix)
    }

    /// Make `temp` durable, run `before_commit`, and rename it over `target`.
    ///
    /// `before_commit` runs once the new bytes are on disk but **before** the
    /// rename: the seam for work that must see the file the write is about to
    /// replace (the store's rolling backup). The old file is still in place
    /// when it runs, and still in place if it throws, so neither failure can
    /// leave a half-written file. A failed commit removes its temp.
    pub(crate) fn commit(
        &self,
        temp: &Path,
        target: &Path,
        before_commit: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let outcome = (|| {
            // The sync handle carries write access: Windows' `FlushFileBuffers`
            // needs `GENERIC_WRITE` and answers "Access is denied" on the
            // read-only handle `File::open` would give, and that is every
            // commit, on every write, since the backup rides this path.
            let file = OpenOptions::new().write(true).open(temp).map_err(|error| {
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

    /// Flush the directory's entries: a rename is durable only once the entry
    /// it changed is synced. An unopenable directory is a failed guarantee,
    /// not a tolerated absence, so the caller cannot proceed unconfirmed.
    #[cfg(unix)]
    pub(crate) fn fsync_directory(&self) -> Result<()> {
        let directory = File::open(&self.root).map_err(|error| {
            StoreError::Io(format!(
                "could not open directory {}: {error}",
                self.root.display()
            ))
        })?;
        directory.sync_all().map_err(|error| {
            StoreError::Io(format!(
                "could not fsync directory {}: {error}",
                self.root.display()
            ))
        })
    }

    /// Windows has no directory fsync; the rename is still the atomic step.
    #[cfg(not(unix))]
    pub(crate) fn fsync_directory(&self) -> Result<()> {
        Ok(())
    }

    /// The exclusive `flock`, behind this process's queue, with the bounded
    /// retry for a holder outside it. The returned guard releases both when it
    /// drops.
    fn acquire(&self) -> Result<LockGuard> {
        fs::create_dir_all(&self.root).map_err(|error| {
            StoreError::Io(format!(
                "could not create the directory {}: {error}",
                self.root.display()
            ))
        })?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_file)
            .map_err(|error| {
                StoreError::Io(format!(
                    "could not open the lock file {}: {error}",
                    self.lock_file.display()
                ))
            })?;
        let key = queue_key(&self.lock_file)?;
        let queue = queue_for(&key);
        // One window for the whole wait, as before the queue: the turn in the
        // queue and then the `flock`. The queue only decides who is served
        // first, never how long a writer may wait.
        let deadline = Instant::now() + LOCK_TIMEOUT;
        let ticket = match queue.take(deadline) {
            Ok(ticket) => ticket,
            Err(error) => {
                forget(&key, &queue);
                return Err(error);
            }
        };
        let held = LockGuard {
            file: Some(file),
            queue,
            ticket,
            key,
        };
        loop {
            match held.file().try_lock() {
                Ok(()) => {
                    return Ok(held);
                }
                Err(TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return Err(StoreError::Locked);
                    }
                    // Yield, do not sleep. A holder outside this process
                    // releases and reacquires within tens of microseconds, so a
                    // retry that pauses for milliseconds misses the gap between
                    // two writes and can be excluded for the whole queue. The
                    // deadline still bounds it.
                    thread::yield_now();
                }
                Err(TryLockError::Error(error)) => {
                    return Err(StoreError::Io(format!(
                        "could not lock {}: {error}",
                        self.lock_file.display()
                    )));
                }
            }
        }
    }

    /// Remove `<prefix>*<suffix>` files older than [`ORPHAN_MAX_AGE`]: flat for
    /// the store, whose temps sit beside the database, and recursive for the
    /// bikeshed, whose temps sit beside each note in the tree.
    fn sweep_orphans(&self) {
        let cutoff = SystemTime::now()
            .checked_sub(ORPHAN_MAX_AGE)
            .unwrap_or(UNIX_EPOCH);
        if self.recursive {
            self.sweep_tree(&self.root, cutoff);
            return;
        }
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            self.sweep_entry(&entry, cutoff);
        }
    }

    /// Walk `directory` and sweep every stale temp under it. A missing or
    /// unreadable directory is an empty level, never an error.
    fn sweep_tree(&self, directory: &Path, cutoff: SystemTime) {
        let Ok(entries) = fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                self.sweep_tree(&entry.path(), cutoff);
            } else {
                self.sweep_entry(&entry, cutoff);
            }
        }
    }

    /// Remove one entry when it is this domain's temp and older than the cutoff.
    fn sweep_entry(&self, entry: &fs::DirEntry, cutoff: SystemTime) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { return };
        if !name.starts_with(&self.temp_prefix) || !name.ends_with(TEMP_SUFFIX) {
            return;
        }
        let Ok(metadata) = entry.metadata() else {
            return;
        };
        let Ok(modified) = metadata.modified() else {
            return;
        };
        if modified < cutoff {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// The lock held for the length of one write: the sidecar's `flock` and the
/// queue ticket that release hands on. Dropping it closes the file, which is
/// what releases the `flock`, and only then serves the next ticket.
pub(crate) struct LockGuard {
    file: Option<File>,
    queue: Arc<Queue>,
    ticket: u64,
    key: PathBuf,
}

impl LockGuard {
    fn file(&self) -> &File {
        self.file.as_ref().expect("the held lock keeps its file")
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        // Closing the handle releases the `flock`, and it has to be closed
        // before the next ticket is served: a waiter woken first would find the
        // lock still held and spend the window racing the release.
        drop(self.file.take());
        self.queue.release(self.ticket);
        forget(&self.key, &self.queue);
    }
}

/// A temp path beside `target`, unique across processes and calls.
pub(crate) fn sibling(target: &Path, prefix: &str) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{prefix}{}{TEMP_SUFFIX}", stamp()))
}

/// A name no other writer in this process or another can pick at the same
/// nanosecond: the process id, the wall clock and a counter.
fn stamp() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let serial = NEXT.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}-{serial}", std::process::id())
}
