//! Tandem's notes surface (`Bikeshed.swift`): a plain folder of markdown notes
//! shared by the app and agents on the same machine, the sibling of the store.
//!
//! Ported from `Sources/TandemCore/Bikeshed*.swift` and `LockedDirectory.swift`:
//!
//! - **Identity and naming** — a note is any file ending `.md`, case-insensitively.
//!   Hidden files are ignored everywhere except in the tree when the human reveals
//!   them; non-`.md` files are attachments. The app only *creates* names free of
//!   `#` and `|`, the wikilink grammar's separators; pre-existing violators on
//!   disk are still shown and edited.
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
//! The derived readers (search, backlinks, health, front page, stats) recompute
//! from the file set; there is no stored index.

pub mod capture;
pub mod daily;
pub mod front;
pub mod health;
pub mod session_notes;
pub mod stats;
pub mod wikilink;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::operation::Operation;
use crate::domain::ticket_to_note::Notes;
use crate::store::StoreError;
use crate::store::locked::LockedDirectory;

use self::wikilink::{LinkOccurrence, LinkResolution, NoteIndex, link_occurrences, rewrite_links};

pub use self::capture::CaptureKind;
pub use self::daily::{DailyNote, Day};
pub use self::front::{ProjectFront, ScaffoldPage};
pub use self::health::{BikeshedHealth, BikeshedHealthCounts, BrokenLink};
pub use self::stats::{BikeshedProjectStats, BikeshedStats, BikeshedStatsTotals};
pub use self::wikilink::{LinkResolution as Resolution, Wikilink};

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
    /// an empty path, any empty component, and any `.` or `..` component.
    pub fn url(root: &Path, path: &str) -> Option<PathBuf> {
        let components: Vec<&str> = path.split('/').collect();
        if components.is_empty() {
            return None;
        }
        if components
            .iter()
            .any(|component| component.is_empty() || *component == "." || *component == "..")
        {
            return None;
        }
        let mut url = standardize(root);
        for component in components {
            url.push(component);
        }
        Some(url)
    }
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

/// The vault: the directory, the sidecar lockfile and the reveal flag.
#[derive(Debug, Clone)]
pub struct Bikeshed {
    /// The vault root (created lazily).
    pub directory: PathBuf,
    /// The sidecar lockfile, outside the vault (created if missing).
    pub lock_file: PathBuf,
    /// Whether the tree reveals the hidden layer. The flag decides what the tree
    /// draws, never what the curated graph is: the linkable note set always prunes
    /// the hidden layer.
    pub shows_hidden_files: bool,
    storage: LockedDirectory,
}

impl Bikeshed {
    /// A vault rooted at `directory`, with the sibling `bikeshed.lock` as its
    /// sidecar (the tests' and the app's explicit path).
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self::with_reveal(directory, false)
    }

    pub fn with_reveal(directory: impl Into<PathBuf>, shows_hidden_files: bool) -> Self {
        let directory = standardize(&directory.into());
        let lock_file = directory
            .parent()
            .map(|parent| parent.join("bikeshed.lock"))
            .unwrap_or_else(|| PathBuf::from("bikeshed.lock"));
        Self::with_lock_file(directory, lock_file, shows_hidden_files)
    }

    /// A vault with an explicit sidecar lockfile: the default (chosen) folder
    /// keeps its lock in Tandem's home rather than beside an arbitrary folder.
    pub fn with_lock_file(
        directory: impl Into<PathBuf>,
        lock_file: impl Into<PathBuf>,
        shows_hidden_files: bool,
    ) -> Self {
        let directory = standardize(&directory.into());
        let lock_file = lock_file.into();
        let storage = LockedDirectory::for_bikeshed(directory.clone(), lock_file.clone());
        Self {
            directory,
            lock_file,
            shows_hidden_files,
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

    /// Whether `path` names something inside the hidden layer: any component
    /// starts with a dot.
    pub fn is_hidden_path(path: &str) -> bool {
        path.split('/').any(|component| component.starts_with('.'))
    }

    /// The full tree: folders first, alphabetical within each level. Hidden
    /// entries are drawn only when the reveal flag is on.
    pub fn tree(&self) -> Vec<BikeshedNode> {
        self.nodes(&self.directory, "", self.shows_hidden_files)
    }

    fn nodes(&self, directory: &Path, relative: &str, include_hidden: bool) -> Vec<BikeshedNode> {
        let Ok(entries) = fs::read_dir(directory) else {
            return Vec::new();
        };
        let mut folders = Vec::new();
        let mut notes = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if is_hidden(&name) && !include_hidden {
                continue;
            }
            let path = if relative.is_empty() {
                name.clone()
            } else {
                format!("{relative}/{name}")
            };
            let Ok(metadata) = fs::metadata(entry.path()) else {
                continue;
            };
            if metadata.is_dir() {
                folders.push(BikeshedNode {
                    name: name.clone(),
                    path: path.clone(),
                    kind: NodeKind::Folder,
                    children: self.nodes(&entry.path(), &path, include_hidden),
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

    /// The linkable note set: every `.md` path, hidden layer always pruned.
    pub fn note_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        collect_notes(&self.nodes(&self.directory, "", false), &mut paths);
        paths
    }

    // MARK: - Path resolution

    /// Resolve a bikeshed-root-relative path to a path inside the vault.
    pub fn file_url(&self, path: &str) -> Option<PathBuf> {
        BikeshedPath::url(&self.directory, path)
    }

    /// The bikeshed-root-relative path of a path inside the vault, in the vault's
    /// own spelling.
    fn relative_path(&self, url: &Path) -> String {
        let root = standardize(&self.directory);
        let full = standardize(url);
        match full.strip_prefix(&root) {
            Ok(relative) => relative.to_string_lossy().to_string(),
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
                || (is_hidden(component) && !self.shows_hidden_files)
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

    /// `write(path, text)`: the autosave path, an atomic write under the lock.
    /// A new path gets the full naming validation; an existing path (including a
    /// pre-existing violator) is edited in place.
    pub fn write(&self, path: &str, text: &str) -> DomainResult<Operation<WriteRecord>> {
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
            self.atomic_write(bytes, &url)?;
            Ok(Operation {
                value: record.clone(),
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
            let mut dest_url = self.directory.clone();
            for component in &destination {
                dest_url.push(component);
            }
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

    /// `delete(path)`: remove the note, or the folder recursively. Links to it
    /// become broken; nothing is rewritten.
    pub fn delete(&self, path: &str) -> DomainResult<Operation<()>> {
        let Some(url) = self.file_url(path) else {
            return Err(DomainError::InvalidName(path.to_string()));
        };
        self.locked(|| {
            let metadata =
                fs::metadata(&url).map_err(|_| DomainError::UnknownNote(path.to_string()))?;
            let removed = if metadata.is_dir() {
                fs::remove_dir_all(&url)
            } else {
                fs::remove_file(&url)
            };
            removed.map_err(|error| {
                DomainError::BikeshedIo(format!("could not delete \"{path}\": {error}"))
            })?;
            Ok(Operation {
                value: (),
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
            let first = link_occurrences(&content.text, &notes)
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
    pub fn resolve(&self, target: &str) -> LinkResolution {
        NoteIndex::new(&self.note_paths()).resolve(target)
    }

    /// `links(text)`: every wikilink occurrence in a note's text.
    pub fn links(&self, text: &str) -> Vec<LinkOccurrence> {
        link_occurrences(text, &self.note_paths())
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

impl Notes for Bikeshed {
    fn write_new(&self, path: &str, text: &str) -> Result<String, String> {
        self.validate_new_name(path)
            .map_err(|error| error.message())?;
        self.locked(|| {
            // A folder that is really a file blocks every candidate: the path is
            // uncreatable.
            if let Some(taken) = self.file_collision(path, false) {
                return Err(DomainError::NoteAlreadyExists(taken));
            }
            let mut candidate = path.to_string();
            let mut suffix = 2;
            while self.file_url(&candidate).is_some_and(|url| url.exists()) {
                candidate = variant(path, suffix);
                suffix += 1;
            }
            let Some(url) = self.file_url(&candidate) else {
                return Err(DomainError::InvalidName(candidate));
            };
            self.atomic_write(text.as_bytes(), &url)?;
            Ok(candidate)
        })
        .map_err(|error| error.message())
    }

    fn delete(&self, path: &str) -> Result<(), String> {
        self.delete(path)
            .map(|_| ())
            .map_err(|error| error.message())
    }
}

// MARK: - Shared helpers

fn translate_store(error: StoreError) -> DomainError {
    match error {
        StoreError::Locked => DomainError::BikeshedLocked,
        other => DomainError::BikeshedIo(other.to_string()),
    }
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

fn is_note_name(name: &str) -> bool {
    name.to_lowercase().ends_with(".md")
}

fn strip_md(name: &str) -> &str {
    if name.len() >= 3 && name[name.len() - 3..].eq_ignore_ascii_case(".md") {
        &name[..name.len() - 3]
    } else {
        name
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

/// A `path` with `-<suffix>` before its `.md`, for `write_new`'s free variant.
fn variant(path: &str, suffix: u32) -> String {
    match path.strip_suffix(".md") {
        Some(stem) => format!("{stem}-{suffix}.md"),
        None => format!("{path}-{suffix}"),
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
