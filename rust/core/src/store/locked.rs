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

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{Result, StoreError};

/// How long a writer waits for the sidecar lock before answering "locked".
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
        let file = self.acquire()?;
        self.sweep_orphans();
        Ok(LockGuard { _file: file })
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

    /// The exclusive `flock` with bounded retry. The returned file holds the
    /// lock; dropping it releases.
    fn acquire(&self) -> Result<File> {
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
        let deadline = Instant::now() + LOCK_TIMEOUT;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(file),
                Err(TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return Err(StoreError::Locked);
                    }
                    // Yield, do not sleep. The holder releases and reacquires
                    // the lock within tens of microseconds, so a retry that
                    // pauses for milliseconds misses the gap between two writes
                    // and can be excluded for the whole queue: measured on the
                    // Windows runner, a waiter answered `Locked` after the full
                    // window while the lock stood free between every write, and
                    // on Ubuntu it waited 95 ms of a 117 ms queue. Yielding
                    // competes for the gap, so a waiter is turned away for about
                    // one write and not for the queue, and the deadline still
                    // bounds it.
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

/// The lock held for the length of one write. Dropping it closes the sidecar,
/// which releases the `flock`.
pub(crate) struct LockGuard {
    _file: File,
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
