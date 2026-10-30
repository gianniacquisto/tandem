//! A throwaway vault for the bikeshed suites: one directory per sandbox under
//! the temp dir, removed on drop, with the sibling `bikeshed.lock` inside the
//! sandbox's own parent so no two suites share a lock.
//!
//! Nothing here touches `~/.tandem`. The vault directory is **not** created up
//! front: a missing bikeshed is a valid empty bikeshed, and several suites rely
//! on that.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::bikeshed::Bikeshed;

static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

pub struct Vault {
    root: PathBuf,
}

impl Vault {
    pub fn new(name: &str) -> Self {
        let serial = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
        let parent = std::env::temp_dir().join(format!(
            "tandem-bikeshed-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&parent);
        Self {
            root: parent.join("bikeshed"),
        }
    }

    /// The vault root (`<parent>/bikeshed`).
    pub fn dir(&self) -> &Path {
        &self.root
    }

    /// The sandbox parent, which holds the sidecar lockfile.
    pub fn parent(&self) -> &Path {
        self.root.parent().expect("the vault has a parent")
    }

    pub fn bikeshed(&self) -> Bikeshed {
        Bikeshed::new(&self.root)
    }

    pub fn revealed(&self) -> Bikeshed {
        Bikeshed::with_reveal(&self.root, true)
    }

    pub fn lock_path(&self) -> PathBuf {
        self.parent().join("bikeshed.lock")
    }

    /// Write a file straight to disk, bypassing the core.
    pub fn stage(&self, relative: &str, text: &str) {
        let url = self.root.join(relative);
        fs::create_dir_all(url.parent().expect("a file has a parent"))
            .expect("the fixture folders are creatable");
        fs::write(&url, text).expect("the fixture note is writable");
    }

    pub fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root.join(relative)).expect("the note is readable")
    }

    pub fn exists(&self, relative: &str) -> bool {
        self.root.join(relative).exists()
    }

    /// Pin a file's mtime, so a freshness or stats literal is deterministic.
    pub fn set_modified(&self, relative: &str, at: SystemTime) {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(self.root.join(relative))
            .expect("the note opens for its stamp");
        file.set_modified(at).expect("the mtime is settable");
    }

    /// Copy a committed fixture vault into this sandbox's root, so a suite reads
    /// the fixture without ever writing to it.
    pub fn copy_from(&self, source: &Path) {
        fn copy(source: &Path, target: &Path) {
            fs::create_dir_all(target).expect("the target folder is creatable");
            for entry in fs::read_dir(source)
                .expect("the fixture folder is readable")
                .flatten()
            {
                let path = entry.path();
                let destination = target.join(entry.file_name());
                if path.is_dir() {
                    copy(&path, &destination);
                } else {
                    fs::copy(&path, &destination).expect("the fixture file copies");
                }
            }
        }
        copy(source, &self.root);
    }
}

impl Drop for Vault {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.parent());
    }
}

/// A whole-second instant the suites share.
pub fn instant(seconds: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds as u64)
}

/// The fixed instant the capture and stats suites spell out
/// (2026-10-18T14:02:11Z).
pub const INSTANT: i64 = 1_792_332_131;

/// SHA-256 of the text, lowercase hex.
pub fn sha256(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
