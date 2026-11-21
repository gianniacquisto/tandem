//! Tandem's notes surface: a plain folder of markdown notes
//! shared by the app and agents on the same machine, the sibling of the store.
//!
//! - **Identity and naming** — a note is any file ending `.md`, case-insensitively.
//!   Hidden files are ignored everywhere; non-`.md` files are attachments. The
//!   app only *creates* names free of `#` and `|`, the wikilink grammar's
//!   separators; pre-existing violators on disk are still shown and edited.
//! - **Frontmatter** — tolerated, opaque: the core never parses note text.
//! - **Locking** — writes take an exclusive `flock` on the sidecar lockfile
//!   (`<home>/bikeshed.lock`, outside the vault, ADR 0011), held across the whole
//!   operation, multi-file ones included. Readers take no lock.
//! - **Atomic writes** — same-directory temp + fsync + `rename(2)`; orphaned temps
//!   older than 60 s are swept under the lock.
//! - **Rename and move** — one locked transaction: rewrite the referencing notes
//!   first (per-file atomic), then one `rename(2)`. No journal (ADR 0005).
//!
//! A missing bikeshed folder is an empty bikeshed, created lazily, never an error.
//! The derived readers (search, backlinks, health, stats) recompute from the
//! file set; there is no stored index.

pub mod health;
pub(crate) mod locked;
pub mod stats;
pub mod wikilink;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::operation::Operation;
use crate::store::StoreError;

use self::locked::LockedDirectory;

use self::wikilink::{
    LinkOccurrence, LinkResolution, NoteIndex, link_occurrences, rewrite_links, rewrite_typed_links,
};

pub use self::health::{
    BikeshedHealth, BikeshedHealthCounts, BrokenLink, MissingFolder, UnmatchedFolder, label_folder,
    project_folder,
};
pub use self::stats::{BikeshedProjectStats, BikeshedStats, BikeshedStatsTotals};
pub use self::wikilink::{BoardNames, BoardProject, LinkResolution as Resolution, Wikilink};

/// The bikeshed path-hygiene rule (ADR 0008): the one place that decides whether
/// a bikeshed-root-relative path is legal or an escape.
pub struct BikeshedPath;

impl BikeshedPath {
    /// The note's name: its file name without the `.md` extension.
    pub fn name(path: &str) -> String {
        let name = path.rsplit('/').next().unwrap_or(path);
        strip_md(name).to_string()
    }

    /// The note's folder, or `root` for a note at the vault's top.
    pub fn folder(path: &str) -> String {
        match path.rfind('/') {
            Some(slash) => path[..slash].to_string(),
            None => "root".to_string(),
        }
    }

    /// `path` as a filesystem path under `root`, or `None` for a vault escape:
    /// an empty path, any empty component, any `.` or `..` component, and any
    /// component the host would reinterpret as a path of its own.
    ///
    /// Each string component must be exactly one [`Component::Normal`]. A `\` is
    /// a Windows separator and `C:` a drive, so `PathBuf::push` would replace
    /// part of the root or append `..` segments the OS then resolves: a
    /// component carrying either is refused here, on every host, so appending
    /// it can never leave `root`.
    pub fn url(root: &Path, path: &str) -> Option<PathBuf> {
        let components: Vec<&str> = path.split('/').collect();
        if components.is_empty() {
            return None;
        }
        for component in &components {
            if component.is_empty() || *component == "." || *component == ".." {
                return None;
            }
            if component.contains('\\') {
                return None;
            }
            let mut parts = Path::new(component).components();
            if !matches!(
                (parts.next(), parts.next()),
                (Some(Component::Normal(_)), None)
            ) {
                return None;
            }
        }
        let root = standardize(root);
        if passes_through_a_link(&root, &components) {
            return None;
        }
        let mut url = root;
        for component in components {
            url.push(component);
        }
        Some(url)
    }
}

/// True when a component below `root` that exists is a link (ADR 0008). The
/// walk starts under the root, so a root that is itself a link still works. A
/// component that does not exist ends the walk: nothing below it can exist.
fn passes_through_a_link(root: &Path, components: &[&str]) -> bool {
    let mut cursor = root.to_path_buf();
    for component in components {
        cursor.push(component);
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if is_link(&metadata) => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    false
}

/// True for a symbolic link, and on Windows also a junction (a reparse point
/// that `is_symlink` does not report). Callers pass `symlink_metadata`.
pub(crate) fn is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// A node in the bikeshed tree: a folder or a `.md` note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BikeshedNode {
    pub name: String,
    pub path: String,
    pub kind: NodeKind,
    pub children: Vec<BikeshedNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Folder,
    Note,
}

/// A note's on-disk state: the text, its content hash and its mtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteContent {
    pub text: String,
    /// SHA-256 of the on-disk bytes, lowercase hex.
    pub hash: String,
    pub modified_at: SystemTime,
}

/// What `write` put on disk: the path and the hash of the written bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteRecord {
    pub path: String,
    pub hash: String,
}

/// What a `move` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveOutcome {
    pub path: String,
    pub old_path: String,
    pub is_folder: bool,
    pub moved_notes: Vec<MovedNote>,
}

/// One note whose path changed in a move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovedNote {
    pub old_path: String,
    pub new_path: String,
}

/// One backlink row: a note containing a wikilink that resolves to the target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlink {
    pub path: String,
    pub line: String,
}

/// One search result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub path: String,
    pub display: String,
}

/// One orphaned temp a killed write left beside a note.
///
/// The atomic write renames into place, so a temp that survives a crash holds
/// the bytes that were about to land. The next locked operation sweeps one
/// older than a minute; until then the app names it as a recovery, never as an
/// error: nothing is lost, and nothing has to be clicked away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    /// The temp's bikeshed-root-relative path.
    pub file: String,
    pub bytes: u64,
    pub modified_at: SystemTime,
}

/// The vault's trash, a hidden folder the tree, search, health and backlink
/// walks skip: a deleted note or folder sits at `.trash/<stamp>/<path>`.
const TRASH_DIR: &str = ".trash";

/// How long a trash entry is kept: the next delete past this age sweeps it.
const TRASH_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// What a `delete` left in the trash: the id `restore` takes (`<stamp>/<original
/// path>`) and the path the entry was deleted from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashEntry {
    pub id: String,
    pub path: String,
}

/// The vault: the directory and the sidecar lockfile.
#[derive(Debug, Clone)]
pub struct Bikeshed {
    /// The vault root (created lazily).
    pub directory: PathBuf,
    /// The sidecar lockfile, outside the vault (created if missing).
    pub lock_file: PathBuf,
    storage: LockedDirectory,
}

impl Bikeshed {
    /// A vault rooted at `directory`, with the sibling `bikeshed.lock` as its
    /// sidecar (the tests' and the app's explicit path).
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        let directory = standardize(&directory.into());
        let lock_file = directory
            .parent()
            .map(|parent| parent.join("bikeshed.lock"))
            .unwrap_or_else(|| PathBuf::from("bikeshed.lock"));
        Self::with_lock_file(directory, lock_file)
    }

    /// Test-facing: this vault with its own lock window, so a contention case
    /// on a slow runner does not race the production 2 s (`LOCK_TIMEOUT`,
    /// which stays 2 s).
    #[cfg(test)]
    pub(crate) fn with_lock_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.storage = self.storage.with_timeout(timeout);
        self
    }

    /// A vault with an explicit sidecar lockfile: the default (chosen) folder
    /// keeps its lock in Tandem's home rather than beside an arbitrary folder.
    pub fn with_lock_file(directory: impl Into<PathBuf>, lock_file: impl Into<PathBuf>) -> Self {
        let directory = standardize(&directory.into());
        let lock_file = lock_file.into();
        let storage = LockedDirectory::for_bikeshed(directory.clone(), lock_file.clone());
        Self {
            directory,
            lock_file,
            storage,
        }
    }

    // MARK: - Locked write primitive

    /// Run `body` under the exclusive vault lock, translating the storage
    /// primitive's failures into the vault's vocabulary.
    fn locked<T>(&self, body: impl FnOnce() -> DomainResult<T>) -> DomainResult<T> {
        let _guard = self.storage.lock().map_err(translate_store)?;
        body()
    }

    /// Write `bytes` to `url` atomically. Call only while holding the lock.
    fn atomic_write(&self, bytes: &[u8], url: &Path) -> DomainResult<()> {
        self.storage
            .atomic_write(bytes, url)
            .map_err(translate_store)
    }

    /// The first component of `path` that exists as a regular file, or `None`.
    /// `including_note` decides whether the note itself is a candidate.
    fn file_collision(&self, path: &str, including_note: bool) -> Option<String> {
        let components: Vec<&str> = path.split('/').collect();
        let to_check: &[&str] = if including_note {
            &components
        } else {
            &components[..components.len().saturating_sub(1)]
        };
        let mut cursor = self.directory.clone();
        let mut prefix = String::new();
        for name in to_check {
            if prefix.is_empty() {
                prefix.push_str(name);
            } else {
                prefix.push('/');
                prefix.push_str(name);
            }
            cursor.push(name);
            if let Ok(metadata) = fs::metadata(&cursor)
                && !metadata.is_dir()
            {
                return Some(prefix);
            }
        }
        None
    }

    // MARK: - The walk

    /// The full tree: folders first, alphabetical within each level. Hidden
    /// entries are never drawn: there is no hidden layer to reveal.
    pub fn tree(&self) -> Vec<BikeshedNode> {
        self.nodes(&self.directory, "")
    }

    fn nodes(&self, directory: &Path, relative: &str) -> Vec<BikeshedNode> {
        let Ok(entries) = fs::read_dir(directory) else {
            return Vec::new();
        };
        let mut folders = Vec::new();
        let mut notes = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if is_hidden(&name) {
                continue;
            }
            let path = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if is_link(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                folders.push(BikeshedNode {
                    name: name.clone(),
                    path: path.clone(),
                    kind: NodeKind::Folder,
                    children: self.nodes(&entry.path(), &path),
                });
            } else if is_note_name(&name) {
                notes.push(BikeshedNode {
                    name,
                    path,
                    kind: NodeKind::Note,
                    children: Vec::new(),
                });
            }
        }
        folders.sort_by_key(|node| sort_key(&node.name));
        notes.sort_by_key(|node| sort_key(&node.name));
        folders.extend(notes);
        folders
    }

    /// The linkable note set: every `.md` path, hidden entries pruned.
    pub fn note_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        collect_notes(&self.nodes(&self.directory, ""), &mut paths);
        paths
    }

    /// The orphaned temps a killed write left, read-only and lock-free.
    ///
    /// A temp is found by the atomic write's own prefix and suffix and carries
    /// no note path, so the recovery names the file; the panel shows it beside
    /// the note it will be swept from. Hidden folders are included: a capture's
    /// interrupted write is still a human's words.
    pub fn recoveries(&self) -> Vec<Recovery> {
        let mut found = Vec::new();
        collect_recoveries(&self.directory, "", &mut found);
        found.sort_by(|left, right| left.file.cmp(&right.file));
        found
    }

    // MARK: - Path resolution

    /// Resolve a bikeshed-root-relative path to a path inside the vault.
    pub fn file_url(&self, path: &str) -> Option<PathBuf> {
        BikeshedPath::url(&self.directory, path)
    }

    /// The bikeshed-root-relative path of a path inside the vault, in the vault's
    /// own spelling: `/`-separated on every platform, never the host's `\`.
    fn relative_path(&self, url: &Path) -> String {
        let root = standardize(&self.directory);
        let full = standardize(url);
        match full.strip_prefix(&root) {
            Ok(relative) => slash_path(relative),
            Err(_) => full
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
        }
    }

    /// The naming rules for a note the app is about to create.
    fn validate_new_name(&self, path: &str) -> DomainResult<()> {
        let last = self.validate_components(path)?;
        if !is_note_name(last) {
            return Err(DomainError::InvalidName(path.to_string()));
        }
        Ok(())
    }

    /// The naming rules for a folder the app is about to create.
    fn validate_new_folder_name(&self, path: &str) -> DomainResult<()> {
        self.validate_components(path).map(|_| ())
    }

    fn validate_components<'a>(&self, path: &'a str) -> DomainResult<&'a str> {
        let components: Vec<&str> = path.split('/').collect();
        let Some(last) = components.last() else {
            return Err(DomainError::InvalidName(path.to_string()));
        };
        for component in &components {
            if component.is_empty()
                || *component == "."
                || *component == ".."
                || is_hidden(component)
                || component.contains('#')
                || component.contains('|')
            {
                return Err(DomainError::InvalidName(path.to_string()));
            }
        }
        Ok(last)
    }

    // MARK: - Note read

    /// Read a note's text, hash and mtime. Lock-free.
    fn read_note(&self, path: &str) -> DomainResult<NoteContent> {
        let Some(url) = self.file_url(path) else {
            return Err(DomainError::UnknownNote(path.to_string()));
        };
        let Ok(metadata) = fs::metadata(&url) else {
            return Err(DomainError::UnknownNote(path.to_string()));
        };
        if metadata.is_dir() {
            return Err(DomainError::UnknownNote(path.to_string()));
        }
        let bytes = fs::read(&url)
            .map_err(|_| DomainError::BikeshedIo(format!("could not read \"{path}\"")))?;
        let text = String::from_utf8(bytes.clone())
            .map_err(|_| DomainError::BikeshedIo(format!("note \"{path}\" is not valid UTF-8")))?;
        let modified_at = metadata
            .modified()
            .map_err(|_| DomainError::BikeshedIo(format!("could not stat \"{path}\"")))?;
        Ok(NoteContent {
            text,
            hash: hash(&bytes),
            modified_at,
        })
    }

    // MARK: - list

    /// `list()`: the tree. Lock-free.
    pub fn list(&self) -> DomainResult<Operation<Vec<BikeshedNode>>> {
        Ok(Operation {
            value: self.tree(),
            note: None,
        })
    }

    // MARK: - read

    /// `read(path)`: the note's text, hash and mtime. Lock-free.
    pub fn read(&self, path: &str) -> DomainResult<Operation<NoteContent>> {
        Ok(Operation {
            value: self.read_note(path)?,
            note: None,
        })
    }

    // MARK: - asset

    // MARK: - create

    /// `create(path)`: a new empty note. Parent folders are created; a taken
    /// target is an error, never an overwrite.
    pub fn create(&self, path: &str) -> DomainResult<Operation<String>> {
        self.validate_new_name(path)?;
        let Some(url) = self.file_url(path) else {
            return Err(DomainError::InvalidName(path.to_string()));
        };
        self.locked(|| {
            if let Some(taken) = self.file_collision(path, true) {
                return Err(DomainError::NoteAlreadyExists(taken));
            }
            self.atomic_write(&[], &url)?;
            Ok(Operation {
                value: path.to_string(),
                note: None,
            })
        })
    }

    // MARK: - createFolder

    /// `createFolder(path)`: a new folder, no note. Same hygiene as a note name,
    /// minus the `.md` requirement.
    pub fn create_folder(&self, path: &str) -> DomainResult<Operation<String>> {
        self.validate_new_folder_name(path)?;
        let Some(url) = self.file_url(path) else {
            return Err(DomainError::InvalidName(path.to_string()));
        };
        self.locked(|| {
            if fs::symlink_metadata(&url).is_ok() {
                return Err(DomainError::NoteAlreadyExists(path.to_string()));
            }
            if let Some(taken) = self.file_collision(path, false) {
                return Err(DomainError::NoteAlreadyExists(taken));
            }
            fs::create_dir_all(&url).map_err(|error| {
                DomainError::BikeshedIo(format!("could not create folder \"{path}\": {error}"))
            })?;
            Ok(Operation {
                value: path.to_string(),
                note: None,
            })
        })
    }

    // MARK: - write

    /// `write(path, text, expected_hash)`: the autosave path, an atomic write
    /// under the lock. A new path gets the full naming validation; an existing
    /// path (including a pre-existing violator) is edited in place.
    ///
    /// `expected_hash` is the one conflict rule for every door. `Some(hash)` is
    /// compared with the hash of the file as it is on disk, inside the lock and
    /// before the write, and a mismatch is `StaleNote` with the disk untouched.
    /// For a path with no file yet the expected hash is [`ABSENT_HASH`]. `None`
    /// overwrites without a check.
    pub fn write(
        &self,
        path: &str,
        text: &str,
        expected_hash: Option<&str>,
    ) -> DomainResult<Operation<WriteRecord>> {
        let Some(url) = self.file_url(path) else {
            return Err(DomainError::InvalidName(path.to_string()));
        };
        match fs::metadata(&url) {
            Ok(metadata) if metadata.is_dir() => {
                return Err(DomainError::BikeshedIo(format!(
                    "path \"{path}\" is a folder"
                )));
            }
            Ok(_) => {}
            Err(_) => self.validate_new_name(path)?,
        }
        let bytes = text.as_bytes();
        let record = WriteRecord {
            path: path.to_string(),
            hash: hash(bytes),
        };
        self.locked(|| {
            if let Some(taken) = self.file_collision(path, false) {
                return Err(DomainError::NoteAlreadyExists(taken));
            }
            if let Some(expected) = expected_hash {
                let current = match fs::read(&url) {
                    Ok(on_disk) => hash(&on_disk),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        ABSENT_HASH.to_string()
                    }
                    Err(error) => {
                        return Err(DomainError::BikeshedIo(format!(
                            "could not read \"{path}\": {error}"
                        )));
                    }
                };
                if current != expected {
                    return Err(DomainError::StaleNote(path.to_string()));
                }
            }
            self.atomic_write(bytes, &url)?;
            Ok(Operation {
                value: record.clone(),
                note: None,
            })
        })
    }

    // MARK: - edit

    /// `edit(path, old, new, expected_hash)`: replace the one occurrence of
    /// `old` in an existing note, the read, the match and the atomic write all
    /// under the vault lock, so two edits to one note never lose each other.
    ///
    /// Zero matches and more than one are refusals that leave the file as it
    /// was. An empty `old` appends `new` at the end; a note that already ends
    /// in `new` is left untouched, so a retried append lands once. The record
    /// carries the hash on disk afterwards, never the text.
    pub fn edit(
        &self,
        path: &str,
        old: &str,
        new: &str,
        expected_hash: Option<&str>,
    ) -> DomainResult<Operation<WriteRecord>> {
        let Some(url) = self.file_url(path) else {
            return Err(DomainError::InvalidName(path.to_string()));
        };
        if old.is_empty() && new.is_empty() {
            return Err(DomainError::EditEmpty);
        }
        self.locked(|| {
            let current = self.read_note(path)?;
            if let Some(expected) = expected_hash
                && current.hash != expected
            {
                return Err(DomainError::StaleNote(path.to_string()));
            }
            let edited = if old.is_empty() {
                if current.text.ends_with(new) {
                    return Ok(Operation {
                        value: WriteRecord {
                            path: path.to_string(),
                            hash: current.hash,
                        },
                        note: None,
                    });
                }
                format!("{}{new}", current.text)
            } else {
                let count = current.text.matches(old).count();
                match count {
                    0 => return Err(DomainError::EditNoMatch(path.to_string())),
                    1 => current.text.replacen(old, new, 1),
                    count => {
                        return Err(DomainError::EditAmbiguous {
                            path: path.to_string(),
                            count,
                        });
                    }
                }
            };
            let record = WriteRecord {
                path: path.to_string(),
                hash: hash(edited.as_bytes()),
            };
            self.atomic_write(edited.as_bytes(), &url)?;
            Ok(Operation {
                value: record,
                note: None,
            })
        })
    }

    // MARK: - rename

    /// `rename(from, to)`: change a note's path with the link rewrite (ADR 0005),
    /// in one locked transaction.
    pub fn rename(&self, from: &str, to: &str) -> DomainResult<Operation<String>> {
        self.validate_new_name(to)?;
        let (Some(old_url), Some(new_url)) = (self.file_url(from), self.file_url(to)) else {
            return Err(DomainError::InvalidName(from.to_string()));
        };
        self.locked(|| {
            match fs::metadata(&old_url) {
                Ok(metadata) if !metadata.is_dir() => {}
                _ => return Err(DomainError::UnknownNote(from.to_string())),
            }
            if fs::symlink_metadata(&new_url).is_ok() {
                return Err(DomainError::NoteAlreadyExists(to.to_string()));
            }
            if let Some(taken) = self.file_collision(to, false) {
                return Err(DomainError::NoteAlreadyExists(taken));
            }
            let notes = self.note_paths();
            let mut post_rename: HashSet<String> = notes.iter().cloned().collect();
            post_rename.remove(from);
            post_rename.insert(to.to_string());
            let mut mapping = HashMap::new();
            mapping.insert(from.to_string(), canonical_link_target(to, &post_rename));
            self.apply_link_rewrite(&notes, &mapping)?;
            if let Some(parent) = new_url.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    DomainError::BikeshedIo(format!(
                        "could not create \"{}\": {error}",
                        parent.display()
                    ))
                })?;
            }
            fs::rename(&old_url, &new_url).map_err(|error| {
                DomainError::BikeshedIo(format!("could not rename \"{from}\": {error}"))
            })?;
            Ok(Operation {
                value: to.to_string(),
                note: None,
            })
        })
    }

    // MARK: - renameFolder

    /// `rename_folder(from, to)`: change a folder's path, the folder and every
    /// note in it moving together, with the link rewrite of a move (ADR 0005), in
    /// one locked transaction. `to` may sit under another parent. A target that
    /// differs only in case from `from` is the same folder, renamed in place.
    pub fn rename_folder(&self, from: &str, to: &str) -> DomainResult<Operation<MoveOutcome>> {
        self.validate_new_folder_name(to)?;
        let (Some(old_url), Some(new_url)) = (self.file_url(from), self.file_url(to)) else {
            return Err(DomainError::InvalidName(from.to_string()));
        };
        if is_descendant(&new_url, &old_url) && from.to_lowercase() != to.to_lowercase() {
            return Err(DomainError::MoveCycle {
                path: from.to_string(),
                into: to.to_string(),
            });
        }
        self.locked(|| {
            match fs::metadata(&old_url) {
                Ok(metadata) if metadata.is_dir() => {}
                _ => return Err(DomainError::UnknownNote(from.to_string())),
            }
            let same_folder = from.to_lowercase() == to.to_lowercase();
            if !same_folder {
                if fs::symlink_metadata(&new_url).is_ok() {
                    return Err(DomainError::NoteAlreadyExists(to.to_string()));
                }
                if let Some(taken) = self.file_collision(to, false) {
                    return Err(DomainError::NoteAlreadyExists(taken));
                }
            }
            let notes = self.note_paths();
            let prefix = format!("{from}/");
            let moves: Vec<MovedNote> = notes
                .iter()
                .filter(|note| note.starts_with(&prefix))
                .map(|note| MovedNote {
                    old_path: note.clone(),
                    new_path: format!("{to}/{}", &note[prefix.len()..]),
                })
                .collect();
            let mut post_move: HashSet<String> = notes.iter().cloned().collect();
            for moved in &moves {
                post_move.remove(&moved.old_path);
                post_move.insert(moved.new_path.clone());
            }
            let mut mapping = HashMap::new();
            for moved in &moves {
                mapping.insert(
                    moved.old_path.clone(),
                    canonical_link_target(&moved.new_path, &post_move),
                );
            }
            self.apply_link_rewrite(&notes, &mapping)?;
            if let Some(parent) = new_url.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    DomainError::BikeshedIo(format!(
                        "could not create \"{}\": {error}",
                        parent.display()
                    ))
                })?;
            }
            fs::rename(&old_url, &new_url).map_err(|error| {
                DomainError::BikeshedIo(format!("could not move \"{from}\": {error}"))
            })?;
            Ok(Operation {
                value: MoveOutcome {
                    path: to.to_string(),
                    old_path: from.to_string(),
                    is_folder: true,
                    moved_notes: moves,
                },
                note: None,
            })
        })
    }

    // MARK: - rewriteTypedLinks

    /// `rewrite_typed_links(is_project, from, to)`: after the board renamed a
    /// project or label, point every `[[project:From]]` (or `[[label:From]]`) at
    /// the new name, in one locked pass, per-file atomic (ADR 0004). Returns how
    /// many notes changed. A note that fails to write stops the pass with its
    /// error; the notes already rewritten stay rewritten.
    pub fn rewrite_typed_links(
        &self,
        is_project: bool,
        from: &str,
        to: &str,
    ) -> DomainResult<Operation<usize>> {
        self.locked(|| {
            let mut changed = 0;
            for note_path in self.note_paths() {
                let (Ok(content), Some(url)) =
                    (self.read_note(&note_path), self.file_url(&note_path))
                else {
                    continue;
                };
                let Some(rewritten) = rewrite_typed_links(&content.text, is_project, from, to)
                else {
                    continue;
                };
                self.atomic_write(rewritten.as_bytes(), &url)?;
                changed += 1;
            }
            Ok(Operation {
                value: changed,
                note: None,
            })
        })
    }

    // MARK: - move

    /// `move(path, into)`: move a note or folder into another folder, rewriting
    /// every link that resolves to a moved note. One locked transaction.
    pub fn move_into(&self, source: &str, into: &str) -> DomainResult<Operation<MoveOutcome>> {
        let Some(source_url) = self.file_url(source) else {
            return Err(DomainError::InvalidName(source.to_string()));
        };
        let destination = folder_components(into)?;
        self.locked(|| {
            let source_metadata = fs::metadata(&source_url)
                .map_err(|_| DomainError::UnknownNote(source.to_string()))?;
            let source_is_dir = source_metadata.is_dir();
            let dest_url = if destination.is_empty() {
                self.directory.clone()
            } else {
                self.file_url(into)
                    .ok_or_else(|| DomainError::InvalidName(into.to_string()))?
            };
            match fs::metadata(&dest_url) {
                Ok(metadata) if metadata.is_dir() => {}
                _ => return Err(DomainError::UnknownNote(into.to_string())),
            }
            if source_is_dir && is_descendant(&dest_url, &source_url) {
                return Err(DomainError::MoveCycle {
                    path: source.to_string(),
                    into: into.to_string(),
                });
            }
            let source_rel = self.relative_path(&source_url);
            let dest_rel = self.relative_path(&dest_url);
            let name = source_url
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let new_path = if dest_rel.is_empty() {
                name
            } else {
                format!("{dest_rel}/{name}")
            };
            if new_path == source_rel {
                return Ok(Operation {
                    value: MoveOutcome {
                        path: new_path,
                        old_path: source_rel,
                        is_folder: source_is_dir,
                        moved_notes: Vec::new(),
                    },
                    note: None,
                });
            }
            let Some(new_url) = self.file_url(&new_path) else {
                return Err(DomainError::InvalidName(new_path));
            };
            if fs::symlink_metadata(&new_url).is_ok() {
                return Err(DomainError::NoteAlreadyExists(new_path));
            }
            if let Some(taken) = self.file_collision(&new_path, false) {
                return Err(DomainError::NoteAlreadyExists(taken));
            }
            let notes = self.note_paths();
            let moves: Vec<MovedNote> = if source_is_dir {
                let prefix = format!("{source_rel}/");
                notes
                    .iter()
                    .filter(|note| note.starts_with(&prefix))
                    .map(|note| MovedNote {
                        old_path: note.clone(),
                        new_path: format!("{new_path}/{}", &note[prefix.len()..]),
                    })
                    .collect()
            } else {
                vec![MovedNote {
                    old_path: source_rel.clone(),
                    new_path: new_path.clone(),
                }]
            };
            let mut post_move: HashSet<String> = notes.iter().cloned().collect();
            for moved in &moves {
                post_move.remove(&moved.old_path);
                post_move.insert(moved.new_path.clone());
            }
            let mut mapping = HashMap::new();
            for moved in &moves {
                mapping.insert(
                    moved.old_path.clone(),
                    canonical_link_target(&moved.new_path, &post_move),
                );
            }
            self.apply_link_rewrite(&notes, &mapping)?;
            if let Some(parent) = new_url.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    DomainError::BikeshedIo(format!(
                        "could not create \"{}\": {error}",
                        parent.display()
                    ))
                })?;
            }
            fs::rename(&source_url, &new_url).map_err(|error| {
                DomainError::BikeshedIo(format!("could not move \"{source}\": {error}"))
            })?;
            Ok(Operation {
                value: MoveOutcome {
                    path: new_path,
                    old_path: source_rel,
                    is_folder: source_is_dir,
                    moved_notes: moves,
                },
                note: None,
            })
        })
    }

    // MARK: - delete

    /// `delete(path)`: move the note, or the folder with everything in it, into
    /// the vault's trash (`.trash/<UTC stamp>/<path>`). Links to it become broken;
    /// nothing is rewritten, and `restore` brings it back.
    pub fn delete(&self, path: &str) -> DomainResult<Operation<TrashEntry>> {
        self.delete_at(path, SystemTime::now())
    }

    /// `delete` with the clock passed in: the stamp of the trash entry and the
    /// retention sweep both read `now`, so a test pins them without sleeping.
    pub(crate) fn delete_at(
        &self,
        path: &str,
        now: SystemTime,
    ) -> DomainResult<Operation<TrashEntry>> {
        let Some(url) = self.file_url(path) else {
            return Err(DomainError::InvalidName(path.to_string()));
        };
        if path.split('/').any(is_hidden) {
            return Err(DomainError::InvalidName(path.to_string()));
        }
        let id = format!("{}/{path}", stamp(now));
        let Some(trash_url) = BikeshedPath::url(&self.directory, &format!("{TRASH_DIR}/{id}"))
        else {
            return Err(DomainError::InvalidName(path.to_string()));
        };
        self.locked(|| {
            fs::metadata(&url).map_err(|_| DomainError::UnknownNote(path.to_string()))?;
            // Best effort: a sweep that cannot read the trash must not block a delete.
            sweep_trash(&self.directory.join(TRASH_DIR), now);
            if fs::symlink_metadata(&trash_url).is_ok() {
                return Err(DomainError::NoteAlreadyExists(id.clone()));
            }
            if let Some(parent) = trash_url.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    DomainError::BikeshedIo(format!(
                        "could not create \"{}\": {error}",
                        parent.display()
                    ))
                })?;
            }
            fs::rename(&url, &trash_url).map_err(|error| {
                DomainError::BikeshedIo(format!("could not delete \"{path}\": {error}"))
            })?;
            Ok(Operation {
                value: TrashEntry {
                    id,
                    path: path.to_string(),
                },
                note: None,
            })
        })
    }

    /// `restore(id)`: move a trash entry back to the path it was deleted from.
    /// The id is the entry's `<stamp>/<original path>`; both halves pass the same
    /// path rule as any note, so an id cannot name a file outside the trash or
    /// the vault. A taken path is refused and nothing moves. Returns the path.
    pub fn restore(&self, id: &str) -> DomainResult<Operation<String>> {
        let refused = || DomainError::InvalidName(id.to_string());
        let (stamp_part, original) = id.split_once('/').ok_or_else(refused)?;
        if parse_stamp(stamp_part).is_none()
            || original.is_empty()
            || original.split('/').any(is_hidden)
        {
            return Err(refused());
        }
        let source =
            BikeshedPath::url(&self.directory, &format!("{TRASH_DIR}/{id}")).ok_or_else(refused)?;
        let destination = BikeshedPath::url(&self.directory, original).ok_or_else(refused)?;
        self.locked(|| {
            fs::symlink_metadata(&source).map_err(|_| DomainError::UnknownNote(id.to_string()))?;
            if fs::symlink_metadata(&destination).is_ok() {
                return Err(DomainError::NoteAlreadyExists(original.to_string()));
            }
            if let Some(taken) = self.file_collision(original, true) {
                return Err(DomainError::NoteAlreadyExists(taken));
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    DomainError::BikeshedIo(format!(
                        "could not create \"{}\": {error}",
                        parent.display()
                    ))
                })?;
            }
            fs::rename(&source, &destination).map_err(|error| {
                DomainError::BikeshedIo(format!("could not restore \"{original}\": {error}"))
            })?;
            Ok(Operation {
                value: original.to_string(),
                note: None,
            })
        })
    }

    // MARK: - search

    /// `search(query)`: case-insensitive substring over title and body, one row
    /// per note, sorted by path.
    pub fn search(&self, query: &str) -> DomainResult<Operation<Vec<SearchHit>>> {
        let needle = query.trim();
        if needle.is_empty() {
            return Ok(Operation {
                value: Vec::new(),
                note: None,
            });
        }
        let mut hits = Vec::new();
        for note_path in self.note_paths() {
            let Ok(content) = self.read_note(&note_path) else {
                continue;
            };
            let title_end = content.text.find('\n').unwrap_or(content.text.len());
            let title = &content.text[..title_end];
            let body = &content.text[(title_end + 1).min(content.text.len())..];
            if find_ci(title, needle).is_some() {
                hits.push(SearchHit {
                    path: note_path,
                    display: title.to_string(),
                });
            } else if let Some((start, end)) = find_ci(body, needle) {
                hits.push(SearchHit {
                    path: note_path,
                    display: snippet(body, start, end),
                });
            }
        }
        hits.sort_by_key(|hit| sort_key(&hit.path));
        Ok(Operation {
            value: hits,
            note: None,
        })
    }

    // MARK: - backlinks

    /// `backlinks(path)`: the notes with a wikilink that resolves to `path`, one
    /// row each, with the line of the first resolving link.
    pub fn backlinks(&self, path: &str) -> DomainResult<Operation<Vec<Backlink>>> {
        let Some(url) = self.file_url(path) else {
            return Err(DomainError::InvalidName(path.to_string()));
        };
        match fs::metadata(&url) {
            Ok(metadata) if !metadata.is_dir() => {}
            _ => return Err(DomainError::UnknownNote(path.to_string())),
        }
        let notes = self.note_paths();
        let mut rows = Vec::new();
        for note_path in &notes {
            let Ok(content) = self.read_note(note_path) else {
                continue;
            };
            let first = link_occurrences(&content.text, &notes, &BoardNames::default())
                .into_iter()
                .find(|occurrence| {
                    occurrence.resolution == LinkResolution::Resolved(path.to_string())
                });
            if let Some(occurrence) = first {
                rows.push(Backlink {
                    path: note_path.clone(),
                    line: occurrence.line,
                });
            }
        }
        rows.sort_by_key(|row| sort_key(&row.path));
        Ok(Operation {
            value: rows,
            note: None,
        })
    }

    // MARK: - resolve and scan

    /// `resolve(target)`: where a wikilink target points, live from the file set.
    /// Typed targets (`project:`, `label:`) are broken here: no board names.
    pub fn resolve(&self, target: &str) -> LinkResolution {
        self.resolve_with(target, &BoardNames::default())
    }

    /// `resolve(target)` with the board's names, so a typed link resolves.
    pub fn resolve_with(&self, target: &str, names: &BoardNames) -> LinkResolution {
        NoteIndex::new(&self.note_paths()).resolve(target, names)
    }

    /// `links(text)`: every wikilink occurrence in a note's text.
    pub fn links(&self, text: &str) -> Vec<LinkOccurrence> {
        self.links_with(text, &BoardNames::default())
    }

    /// `links(text)` with the board's names, so a typed link resolves.
    pub fn links_with(&self, text: &str, names: &BoardNames) -> Vec<LinkOccurrence> {
        link_occurrences(text, &self.note_paths(), names)
    }

    // MARK: - The rewrite pass

    /// Rewrite every note whose wikilinks resolve to a moved note. Per-file
    /// atomic; resolution is against the pre-move set.
    fn apply_link_rewrite(
        &self,
        notes: &[String],
        mapping: &HashMap<String, String>,
    ) -> DomainResult<()> {
        if mapping.is_empty() {
            return Ok(());
        }
        for note_path in notes {
            let (Ok(content), Some(url)) = (self.read_note(note_path), self.file_url(note_path))
            else {
                continue;
            };
            let Some(rewritten) = rewrite_links(&content.text, notes, mapping) else {
                continue;
            };
            self.atomic_write(rewritten.as_bytes(), &url)?;
        }
        Ok(())
    }
}

// MARK: - Shared helpers

fn translate_store(error: StoreError) -> DomainError {
    match error {
        StoreError::Locked { .. } => DomainError::BikeshedLocked,
        other => DomainError::BikeshedIo(other.to_string()),
    }
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// The UTC stamp a trash entry folder is named by: `YYYYMMDDTHHMMSSmmmZ`, with
/// milliseconds so two deletes in one second keep their own folders.
fn stamp(at: SystemTime) -> String {
    let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs() as i64;
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let rest = secs.rem_euclid(86_400);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}{:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        since.subsec_millis()
    )
}

/// The time a trash folder's stamp names, or `None` for any other name. The
/// stamp must round-trip, so a date that does not exist is not a stamp.
fn parse_stamp(name: &str) -> Option<SystemTime> {
    let bytes = name.as_bytes();
    if bytes.len() != 19 || bytes[8] != b'T' || bytes[18] != b'Z' {
        return None;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| matches!(index, 8 | 18) || byte.is_ascii_digit())
    {
        return None;
    }
    let number = |range: std::ops::Range<usize>| name[range].parse::<i64>().ok();
    let days = days_from_civil(number(0..4)?, number(4..6)?, number(6..8)?);
    let secs = days * 86_400 + number(9..11)? * 3600 + number(11..13)? * 60 + number(13..15)?;
    let millis = number(15..18)?;
    let at = UNIX_EPOCH + Duration::from_millis(u64::try_from(secs).ok()? * 1000 + millis as u64);
    (stamp(at) == name).then_some(at)
}

/// Days since the Unix epoch to a proleptic Gregorian `(year, month, day)`.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The inverse of [`civil_from_days`].
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Remove the trash folders whose stamp is older than [`TRASH_RETENTION`]. An
/// entry of exactly the retention is kept; anything that is not a stamp, a
/// folder or readable is left alone.
fn sweep_trash(trash: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(trash) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(at) = parse_stamp(&name) else {
            continue;
        };
        if now
            .duration_since(at)
            .is_ok_and(|age| age > TRASH_RETENTION)
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn is_note_name(name: &str) -> bool {
    name.to_lowercase().ends_with(".md")
}

fn strip_md(name: &str) -> &str {
    // Split at the last dot and compare the extension, so a non-ASCII stem is
    // never sliced mid-character: the split is at the dot, not at a byte count.
    match name.rsplit_once('.') {
        Some((stem, extension)) if extension.eq_ignore_ascii_case("md") => stem,
        _ => name,
    }
}

fn sort_key(name: &str) -> (String, String) {
    (name.to_lowercase(), name.to_string())
}

fn collect_notes(nodes: &[BikeshedNode], paths: &mut Vec<String>) {
    for node in nodes {
        if node.kind == NodeKind::Note {
            paths.push(node.path.clone());
        }
        collect_notes(&node.children, paths);
    }
}

/// Walk `directory` for the atomic write's orphaned temps, relative to the
/// vault root. Unreadable entries are skipped: a read never fails on a tree it
/// cannot fully see, exactly like [`Bikeshed::tree`].
fn collect_recoveries(directory: &Path, relative: &str, found: &mut Vec<Recovery>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let child = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if is_link(&metadata) {
            continue;
        }
        if metadata.is_dir() {
            collect_recoveries(&path, &child, found);
            continue;
        }
        if name.starts_with(locked::BIKESHED_TEMP_PREFIX) && name.ends_with(".tmp") {
            found.push(Recovery {
                file: child,
                bytes: metadata.len(),
                modified_at: metadata.modified().unwrap_or(UNIX_EPOCH),
            });
        }
    }
}

/// The expected hash of a note that does not exist yet: a write that expects
/// `"absent"` succeeds only if no file is there. It cannot collide with a real
/// hash, which is 64 hex digits.
pub const ABSENT_HASH: &str = "absent";

/// The hash of the bytes: SHA-256, lowercase hex.
fn hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A path with `.` components dropped and `..` components folded, without
/// touching the filesystem.
fn standardize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A path as the vault spells it: `/`-separated on every platform, because a
/// vault-relative path is the contract's, not the host's. A Windows `\` would
/// make a capture's dedupe key, a move's mapping and a session's notes miss.
pub(crate) fn slash_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// The components of a destination folder path, checked for hygiene. The vault
/// root is the empty path.
fn folder_components(path: &str) -> DomainResult<Vec<String>> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    let components: Vec<String> = path.split('/').map(str::to_string).collect();
    if components
        .iter()
        .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(DomainError::InvalidName(path.to_string()));
    }
    Ok(components)
}

/// True when `url` is `ancestor` itself or sits inside it.
fn is_descendant(url: &Path, ancestor: &Path) -> bool {
    standardize(url).starts_with(standardize(ancestor))
}

/// The canonical target for a moved note: the bare basename when unique in the
/// post-move set, the qualified path when it is not.
fn canonical_link_target(new_path: &str, post_move: &HashSet<String>) -> String {
    let bare = strip_md(new_path.rsplit('/').next().unwrap_or(new_path));
    let bare_lower = bare.to_lowercase();
    let count = post_move
        .iter()
        .filter(|note| {
            strip_md(note.rsplit('/').next().unwrap_or(note)).to_lowercase() == bare_lower
        })
        .count();
    if count == 1 {
        bare.to_string()
    } else {
        strip_md(new_path).to_string()
    }
}

/// The first case-insensitive occurrence of `needle` in `haystack`, as byte
/// offsets. Character-wise, so a multi-byte note cannot slice mid-character.
fn find_ci(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() {
        return None;
    }
    let chars: Vec<(usize, char)> = haystack.char_indices().collect();
    for start in 0..chars.len() {
        let mut matched = 0;
        let mut end = chars[start].0;
        while matched < needle.len() {
            let Some((byte, character)) = chars.get(start + matched) else {
                break;
            };
            if !same_ignoring_case(*character, needle[matched]) {
                break;
            }
            end = byte + character.len_utf8();
            matched += 1;
        }
        if matched == needle.len() {
            return Some((chars[start].0, end));
        }
    }
    None
}

fn same_ignoring_case(left: char, right: char) -> bool {
    left == right || left.to_lowercase().eq(right.to_lowercase())
}

/// The display text for a body search hit: 40 characters either side of the
/// match, whitespace collapsed, ellipsised where the slice cut the text.
fn snippet(text: &str, start: usize, end: usize) -> String {
    const CONTEXT: usize = 40;
    let chars: Vec<char> = text.chars().collect();
    let start_char = text[..start].chars().count();
    let end_char = start_char + text[start..end].chars().count();
    let from = start_char.saturating_sub(CONTEXT);
    let to = (end_char + CONTEXT).min(chars.len());
    let collapsed: String = chars[from..to].iter().collect();
    let collapsed = collapse_whitespace(&collapsed);
    let mut out = String::new();
    if from > 0 {
        out.push('…');
    }
    out.push_str(&collapsed);
    if to < chars.len() {
        out.push('…');
    }
    out
}

fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            in_space = true;
        } else {
            if in_space && !out.is_empty() {
                out.push(' ');
            }
            in_space = false;
            out.push(character);
        }
    }
    out
}
