//! The storage mechanics every writing verb shares: a `flock` sidecar with
//! bounded retry, same-directory temp files with `fsync` + rename, and the
//! orphan sweep that clears a crash's leftovers under the lock.
//!
//! Ported from `LockedDirectory.swift` and its tests (`LockedDirectoryTests`),
//! behaviour for behaviour: the lock is never a suggestion, a write lands whole
//! or not at all, and a temp older than a minute is a crash's, not a writer's.
//! This module is the mechanism only. It owns no schema and no domain
//! vocabulary; the store translates its two failures into the store's words.

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

/// One domain's locked directory: the sidecar lockfile, the temp naming and the
/// tree the orphan sweep walks.
#[derive(Debug, Clone)]
pub(crate) struct LockedDirectory {
    lock_file: PathBuf,
    temp_prefix: String,
    root: PathBuf,
}

impl LockedDirectory {
    pub(crate) fn new(lock_file: PathBuf, temp_prefix: impl Into<String>, root: PathBuf) -> Self {
        Self {
            lock_file,
            temp_prefix: temp_prefix.into(),
            root,
        }
    }

    /// The store's own sidecar: `<home>/tandem.lock` and the home as its root.
    pub(crate) fn for_store(home: &Path) -> Self {
        Self::new(home.join("tandem.lock"), TEMP_PREFIX, home.to_path_buf())
    }

    /// Acquire the exclusive lock and run `body` under it, after sweeping the
    /// tree's stale temps. The lock is released when `body` returns or throws;
    /// `flock` releases on process death, so there is no stale lock to recover.
    pub(crate) fn with_lock<T>(&self, body: impl FnOnce() -> Result<T>) -> Result<T> {
        let _lock = self.acquire()?;
        self.sweep_orphans();
        body()
    }

    /// Write `bytes` to `target` through a same-directory temp, then commit it.
    /// Missing parent folders are created, so a first write into a deeper path
    /// lands the same way a file's did (ADR 0001).
    ///
    /// [`LockedDirectory`]'s bytes convenience has no caller in the SQLite
    /// store yet: the bikeshed (PRT-7) is the file tree that needs it, and the
    /// behavior is kept now because the port is ported, not rewritten
    /// (docs/port.md, parity). Its suite is the seam's own.
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
                    thread::sleep(backoff());
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

    /// Remove `<prefix>*<suffix>` files older than [`ORPHAN_MAX_AGE`] in the
    /// root, flat: the store's temps sit beside the database, and the bikeshed
    /// is another domain's tree (its recursive sweep lands with PRT-7).
    fn sweep_orphans(&self) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        let cutoff = SystemTime::now() - ORPHAN_MAX_AGE;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.starts_with(&self.temp_prefix) || !name.ends_with(TEMP_SUFFIX) {
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

/// The retry's pause, 10 to 50 ms as the Swift primitive has it. A small
/// xorshift keeps two waiters apart without a random-number dependency.
fn backoff() -> Duration {
    static NEXT: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let mut value = NEXT.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    value ^= value >> 12;
    value ^= value << 25;
    value ^= value >> 27;
    Duration::from_micros(10_000 + value % 40_001)
}
