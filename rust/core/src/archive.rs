//! The home as one archive (ADR 0050): export a store and its notes into a
//! `.tar.gz`, import one into an empty home.
//!
//! This module owns the format, the walk, the import's checks, the export
//! permission check ([`may_export`]) and the export record. SQL stays in
//! [`crate::store`]; the CLI and the board's download are faces over these
//! functions and add argument parsing and wording only.
//!
//! **The archive**, in this order: `tandem.db` (one committed generation of the
//! store, scrubbed of secret tables), every regular file under `bikeshed/`, and
//! `manifest.json` last, because the digests are computed while the bytes
//! stream. A cut archive therefore has no manifest and is refused as damaged.
//!
//! **What "consistent" means.** The store is one committed instant; each note is
//! whole, because note writes are atomic renames and each file is opened once and
//! read from that handle; the store and the notes are two nearby instants, not
//! one transaction, and a note edited by hand in another program during an
//! export may be caught either side of the edit.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::permission::{Permission, PrincipalKind, permits};
use crate::store::{self, Actor, Home, Refusal, Store, StoreError};

/// The archive format this build writes and reads.
pub const FORMAT_VERSION: u32 = 1;

const STORE_ENTRY: &str = "tandem.db";
const NOTES_ENTRY: &str = "bikeshed";
const MANIFEST_ENTRY: &str = "manifest.json";

/// The most entries an archive may hold: a bound on a hostile archive's
/// bookkeeping, far above any real vault.
const MAX_ENTRIES: usize = 1_000_000;

/// The largest manifest read: it lists one line per entry.
const MAX_MANIFEST_BYTES: u64 = 256 * 1024 * 1024;

// MARK: - The permission check and the record

/// How the caller reached the home, as the door resolved it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// The operator at the home's own files: the offline CLI.
    Local,
    /// A request on a loopback bind.
    Loopback,
    /// A request on a bind that opted out of authentication, a network the
    /// operator trusts.
    Unauthenticated,
    /// A request carrying the server's token.
    Token,
    /// A member's browser session, of this role.
    MemberSession(store::Role),
    /// A member's personal token, of this role.
    MemberToken(store::Role),
    /// The hosting provider's key.
    Operator,
    /// A principal this build does not know. The check fails closed on it.
    Unknown,
}

/// Who asks to export: how they reached the home and the actor the door
/// resolved. The actor is attribution, never the principal (ADR 0047).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub access: Access,
    pub actor: Actor,
}

/// The door an export came in through, for the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Door {
    Cli,
    Board,
}

impl Door {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Board => "board",
        }
    }
}

/// The one export permission check; the CLI verb and the board's route both
/// call it, and nothing else decides. It is a thin caller of
/// [`crate::permission::permits`] with the `export` permission (CUT-39): the
/// owner (the solo modes, `TANDEM_TOKEN`, and the offline CLI at the home's own
/// files) always may, an admin's browser session may, and a member's session,
/// any personal token and the operator may not. It fails closed on a principal
/// it does not know.
///
/// It stops the one-step bulk export; it does not stop a member who can read
/// every ticket and note from copying them, and it does not bind the offline
/// CLI, which opens the home directly: anyone who can read the home's files could
/// copy them anyway, so file permissions are that boundary.
pub fn may_export(principal: &Principal) -> Result<(), ArchiveError> {
    let kind = match principal.access {
        Access::Local | Access::Loopback | Access::Unauthenticated | Access::Token => {
            PrincipalKind::Owner
        }
        Access::MemberSession(role) => PrincipalKind::MemberSession(role),
        Access::MemberToken(role) => PrincipalKind::MemberToken(role),
        Access::Operator => PrincipalKind::Operator,
        Access::Unknown => PrincipalKind::Unknown,
    };
    if permits(kind, Permission::Export) {
        Ok(())
    } else {
        Err(ArchiveError::NotAllowed)
    }
}

/// The export record: one line for an export that starts, naming the actor, the
/// time and the door and nothing else (never a token, a path or content). It is
/// a `tracing::info!` event, and the line is returned so the CLI, which installs
/// no subscriber, prints the same text to stderr. The board's door also writes a
/// row of the durable access record (CUT-39, `Store::access_record_export`) beside
/// it; the CLI door keeps the line only, because it is read-only and offline.
pub fn record_export(actor: &Actor, door: Door) -> String {
    let actor: String = actor
        .as_str()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let at = store::now();
    let line = format!("export actor={actor} door={} at={at}", door.as_str());
    tracing::info!(target: "tandem::export", actor = %actor, door = door.as_str(), at = %at, "export started");
    line
}

/// Check the permission, then write the record: what a door does before it
/// streams a byte. Returns the record line.
pub fn start_export(principal: &Principal, door: Door) -> Result<String, ArchiveError> {
    may_export(principal)?;
    Ok(record_export(&principal.actor, door))
}

// MARK: - Errors

/// Why an export or an import did not happen. Each message says what failed and
/// the next act, in the shape of the CLI's `home_failure`.
#[derive(Debug)]
pub enum ArchiveError {
    /// The principal may not export.
    NotAllowed,
    /// The store could not be read or copied.
    Store(StoreError),
    /// The target home has data: no merge, no force.
    NotEmpty { home: PathBuf, found: Vec<String> },
    /// The archive's store is a newer build's.
    Newer { version: i64 },
    /// An archive format this build does not read.
    Format { found: u32 },
    /// Cut, corrupted or inconsistent with its manifest.
    Damaged(String),
    /// An entry that must never be unpacked.
    Hostile { entry: String, reason: String },
    /// The unpacked total passed the ceiling.
    TooLarge { limit: u64 },
    /// The store inside is not one this build trusts.
    Refused(String),
    /// A filesystem failure; the message names the act and carries its own next step.
    Io(String),
    /// A write to the export's output failed part way (a broken pipe, a full disk).
    Stream(String),
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAllowed => f.write_str(
                "This principal may not export the home. Ask an admin of this workspace.",
            ),
            Self::Store(error) => write!(
                f,
                "Cannot read the store: {error}. Check the home exists and try again."
            ),
            Self::NotEmpty { home, found } => write!(
                f,
                "Cannot import into {}: the home is not empty ({}). Import never merges and has \
                 no force: point --home at a new, empty directory. (A `tandem serve` started on \
                 this home earlier creates an empty store, which counts.)",
                home.display(),
                found.join(", ")
            ),
            Self::Newer { version } => write!(
                f,
                "Cannot import: the archive's store is a newer version ({version}) than this \
                 build reads ({}). Upgrade tandem, then import again. Nothing was written.",
                store::CURRENT_VERSION
            ),
            Self::Format { found } => write!(
                f,
                "Cannot import: the archive is format {found}, and this build reads format \
                 {FORMAT_VERSION}. Upgrade tandem, then import again. Nothing was written."
            ),
            Self::Damaged(why) => write!(
                f,
                "Cannot import: the archive is damaged ({why}). Export it again from the source \
                 and import the new file. Nothing was written."
            ),
            Self::Hostile { entry, reason } => write!(
                f,
                "Cannot import: the entry {entry} is refused ({reason}). The archive was not \
                 made by `tandem export`; get a fresh export. Nothing was written."
            ),
            Self::TooLarge { limit } => write!(
                f,
                "Cannot import: the archive unpacks to more than the {limit} byte ceiling. Raise \
                 --max-bytes or import a smaller workspace. Nothing was written."
            ),
            Self::Refused(why) => write!(
                f,
                "Cannot import: the archive's store is not trusted ({why}). Export it again \
                 from the source. Nothing was written."
            ),
            Self::Io(message) => write!(
                f,
                "{message} Check the path, the free space and the permissions, then try again."
            ),
            Self::Stream(message) => write!(
                f,
                "{message} The archive was cut off while it was written to standard output, so \
                 some bytes were already sent and what the reader holds is partial. Discard it \
                 and export again."
            ),
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<StoreError> for ArchiveError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

// MARK: - The manifest

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    /// The archive format version.
    format: u32,
    /// The store's schema version when the copy was taken.
    schema_version: i64,
    tandem_version: String,
    created_at: String,
    /// Every other entry, in archive order.
    entries: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ManifestEntry {
    path: String,
    size: u64,
    sha256: String,
}

// MARK: - Export

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSummary {
    /// The compressed bytes written to the sink.
    pub bytes: u64,
    /// Files archived, the manifest excluded.
    pub entries: usize,
}

/// Write the archive of `home` to `out` as a stream; nothing is built in memory
/// and no lock is held across the walk.
pub fn export<W: Write>(home: &Home, out: W) -> Result<ExportSummary, ArchiveError> {
    let store = Store::new(home.clone());
    let copy = store.export_copy()?;
    let mut builder = tar::Builder::new(GzEncoder::new(Counting::new(out), Compression::default()));
    let mut entries = Vec::new();

    let mut file = File::open(copy.path()).map_err(io_error("open the export copy"))?;
    let size = file
        .metadata()
        .map_err(io_error("stat the export copy"))?
        .len();
    entries.push(append(&mut builder, STORE_ENTRY, &mut file, size)?);

    for (relative, path) in walk_notes(&home.directory().join(NOTES_ENTRY))? {
        let mut file = File::open(&path).map_err(io_error(&format!("read the note {relative}")))?;
        let metadata = file
            .metadata()
            .map_err(io_error(&format!("stat the note {relative}")))?;
        if !metadata.is_file() {
            continue;
        }
        entries.push(append(
            &mut builder,
            &format!("{NOTES_ENTRY}/{relative}"),
            &mut file,
            metadata.len(),
        )?);
    }

    let manifest = Manifest {
        format: FORMAT_VERSION,
        schema_version: copy.schema_version(),
        tandem_version: crate::release_version().to_string(),
        created_at: store::now(),
        entries: entries.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| ArchiveError::Io(format!("Cannot write the manifest: {error}.")))?;
    let mut header = header_for(bytes.len() as u64, 0);
    builder
        .append_data(&mut header, MANIFEST_ENTRY, bytes.as_slice())
        .map_err(io_error("write the manifest"))?;

    let gzip = builder
        .into_inner()
        .map_err(io_error("finish the archive"))?;
    let mut counting = gzip.finish().map_err(io_error("finish the archive"))?;
    counting.flush().map_err(io_error("flush the archive"))?;
    Ok(ExportSummary {
        bytes: counting.count,
        entries: entries.len(),
    })
}

/// Append one file: its bytes are read once, hashed as they stream, and its
/// length is checked against the header so a file that changed length mid-read
/// fails the export rather than writing a torn entry.
fn append<W: Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    file: &mut File,
    size: u64,
) -> Result<ManifestEntry, ArchiveError> {
    let mtime = file
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let mut header = header_for(size, mtime);
    let mut reader = Hashing::new(file.take(size));
    builder
        .append_data(&mut header, path, &mut reader)
        .map_err(io_error(&format!("write {path} into the archive")))?;
    if reader.count != size {
        return Err(ArchiveError::Io(format!(
            "Cannot export: {path} changed while it was being read."
        )));
    }
    Ok(ManifestEntry {
        path: path.to_string(),
        size,
        sha256: hex(reader.hasher.finalize().as_slice()),
    })
}

fn header_for(size: u64, mtime: u64) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(size);
    header.set_mode(0o600);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(mtime);
    header
}

/// Every regular file under the notes root, vault-relative and `/`-separated,
/// sorted. Symlinks are neither followed nor archived, and Tandem's own temp
/// files are skipped.
fn walk_notes(root: &Path) -> Result<Vec<(String, PathBuf)>, ArchiveError> {
    let mut found = Vec::new();
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.is_dir() && !crate::bikeshed::is_link(&metadata) => {}
        _ => return Ok(found),
    }
    walk_into(root, "", &mut found)?;
    found.sort();
    Ok(found)
}

fn walk_into(
    directory: &Path,
    prefix: &str,
    found: &mut Vec<(String, PathBuf)>,
) -> Result<(), ArchiveError> {
    let entries = fs::read_dir(directory).map_err(io_error("read the notes folder"))?;
    for entry in entries {
        let entry = entry.map_err(io_error("read the notes folder"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(ArchiveError::Io(format!(
                "Cannot export: a note name under {prefix} is not UTF-8."
            )));
        };
        if is_temp(name) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error("stat a note"))?;
        if crate::bikeshed::is_link(&metadata) {
            continue;
        }
        let relative = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        if metadata.is_dir() {
            walk_into(&entry.path(), &relative, found)?;
        } else if metadata.is_file() {
            found.push((relative, entry.path()));
        }
    }
    Ok(())
}

/// Tandem's own temp files: a note write's and a store copy's.
fn is_temp(name: &str) -> bool {
    name.ends_with(".tmp") && (name.starts_with(".note-") || name.starts_with(".tandem-"))
}

// MARK: - Import

/// What an import restored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub schema_version: i64,
    pub tickets: u64,
    pub notes: usize,
}

/// Restore `input` into `home`, which must be empty. Nothing is written into
/// the home until every check has passed; any refusal or failure removes the
/// temp directory and leaves the home as it was.
///
/// `max_bytes` is a ceiling on the unpacked total, enforced while streaming;
/// `None` is no ceiling (limits are the hosted side's, HOS-9).
pub fn import<R: Read>(
    home: &Home,
    input: R,
    max_bytes: Option<u64>,
) -> Result<ImportSummary, ArchiveError> {
    let directory = home.directory();
    refuse_if_not_empty(directory)?;

    let created_home = !directory.exists();
    home.ensure().map_err(|error| {
        ArchiveError::Io(format!(
            "Cannot create the Tandem home at {}: {error}.",
            directory.display()
        ))
    })?;
    let mut scratch = Scratch {
        directory: directory.join(format!(".tandem-import-{}.tmp", unique())),
        created_home: created_home.then(|| directory.to_path_buf()),
        keep: false,
    };
    make_private_dir(&scratch.directory).map_err(io_error("create the import's temp directory"))?;

    let outcome = unpack_and_commit(directory, &scratch.directory, input, max_bytes);
    if outcome.is_ok() {
        scratch.keep = true;
    }
    drop(scratch);
    outcome
}

/// The temp directory, removed on every path; and the home, when this import
/// created it and then failed.
struct Scratch {
    directory: PathBuf,
    created_home: Option<PathBuf>,
    keep: bool,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
        if !self.keep
            && let Some(home) = &self.created_home
        {
            // Empty after the temp is gone, or it is not ours to remove.
            let _ = fs::remove_dir(home);
        }
    }
}

fn refuse_if_not_empty(directory: &Path) -> Result<(), ArchiveError> {
    let mut found = Vec::new();
    for name in [
        "tandem.db",
        "tandem.db.bak",
        "tandem.db-wal",
        "tandem.db-shm",
        "tandem.db-journal",
    ] {
        if fs::symlink_metadata(directory.join(name)).is_ok() {
            found.push(name.to_string());
        }
    }
    let notes = directory.join(NOTES_ENTRY);
    match fs::symlink_metadata(&notes) {
        Ok(metadata) if metadata.is_dir() => {
            let any = fs::read_dir(&notes)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(true);
            if any {
                found.push("bikeshed/ (has notes)".to_string());
            }
        }
        Ok(_) => found.push("bikeshed (not an empty folder)".to_string()),
        Err(_) => {}
    }
    if found.is_empty() {
        Ok(())
    } else {
        Err(ArchiveError::NotEmpty {
            home: directory.to_path_buf(),
            found,
        })
    }
}

fn unpack_and_commit<R: Read>(
    home: &Path,
    scratch: &Path,
    input: R,
    max_bytes: Option<u64>,
) -> Result<ImportSummary, ArchiveError> {
    let mut archive = tar::Archive::new(GzDecoder::new(input));
    let entries = archive.entries().map_err(damaged)?;

    let mut seen: BTreeMap<String, ManifestEntry> = BTreeMap::new();
    let mut directories: BTreeSet<String> = BTreeSet::new();
    let mut manifest: Option<Vec<u8>> = None;
    let mut total: u64 = 0;
    let mut count = 0usize;

    for entry in entries {
        let mut entry = entry.map_err(damaged)?;
        count += 1;
        if count > MAX_ENTRIES {
            return Err(ArchiveError::TooLarge {
                limit: MAX_ENTRIES as u64,
            });
        }
        let raw = entry.path_bytes().into_owned();
        let shown = display(&raw);
        let hostile = |reason: &str| ArchiveError::Hostile {
            entry: shown.clone(),
            reason: reason.to_string(),
        };
        let kind = entry.header().entry_type();
        let is_dir = match kind {
            tar::EntryType::Regular => false,
            tar::EntryType::Directory => true,
            tar::EntryType::Symlink => return Err(hostile("a symbolic link")),
            tar::EntryType::Link => return Err(hostile("a hard link")),
            _ => return Err(hostile("not a regular file or a directory")),
        };
        let slot = classify(&raw, is_dir).map_err(|reason| hostile(&reason))?;
        let size = entry.size();

        match slot {
            Slot::Directory(relative) => {
                directories.insert(relative.clone());
                if relative.starts_with(&format!("{NOTES_ENTRY}/")) || relative == NOTES_ENTRY {
                    make_private_dir_all(&scratch.join(&relative))
                        .map_err(io_error("unpack a folder"))?;
                }
            }
            Slot::Manifest => {
                if manifest.is_some() {
                    return Err(hostile("a duplicate entry"));
                }
                if size > MAX_MANIFEST_BYTES {
                    return Err(hostile("a manifest too large to be one"));
                }
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).map_err(damaged)?;
                manifest = Some(bytes);
            }
            Slot::File(relative) => {
                if seen.contains_key(&relative) || directories.contains(&relative) {
                    return Err(hostile("a duplicate entry"));
                }
                total = total.saturating_add(size);
                if let Some(limit) = max_bytes
                    && total > limit
                {
                    return Err(ArchiveError::TooLarge { limit });
                }
                let target = scratch.join(&relative);
                if let Some(parent) = target.parent() {
                    make_private_dir_all(parent).map_err(io_error("unpack a folder"))?;
                }
                let mut file = create_private_file(&target).map_err(|error| {
                    if error.kind() == io::ErrorKind::AlreadyExists {
                        hostile("a path that collides with another entry")
                    } else {
                        ArchiveError::Io(format!("Cannot unpack {shown}: {error}."))
                    }
                })?;
                let mut hasher = Sha256::new();
                let mut written: u64 = 0;
                let mut buffer = vec![0u8; 64 * 1024];
                loop {
                    let read = entry.read(&mut buffer).map_err(damaged)?;
                    if read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..read]);
                    file.write_all(&buffer[..read])
                        .map_err(io_error("unpack a file"))?;
                    written += read as u64;
                }
                file.sync_all().map_err(io_error("unpack a file"))?;
                if written != size {
                    return Err(ArchiveError::Damaged(format!("{relative} is cut short")));
                }
                seen.insert(
                    relative.clone(),
                    ManifestEntry {
                        path: relative,
                        size: written,
                        sha256: hex(hasher.finalize().as_slice()),
                    },
                );
            }
        }
    }

    let manifest = manifest.ok_or_else(|| {
        ArchiveError::Damaged("it has no manifest, so it was cut short".to_string())
    })?;
    let manifest: Manifest = serde_json::from_slice(&manifest)
        .map_err(|error| ArchiveError::Damaged(format!("the manifest does not read: {error}")))?;
    if manifest.format != FORMAT_VERSION {
        return Err(ArchiveError::Format {
            found: manifest.format,
        });
    }
    verify_against_manifest(&manifest, &seen)?;
    if !seen.contains_key(STORE_ENTRY) {
        return Err(ArchiveError::Damaged("it has no tandem.db".to_string()));
    }

    let unpacked_store = scratch.join(STORE_ENTRY);
    let inspection = store::inspect_unpacked(&unpacked_store).map_err(|refusal| match refusal {
        Refusal::Newer { version } => ArchiveError::Newer { version },
        Refusal::Damaged(why) => ArchiveError::Damaged(why),
        Refusal::Foreign(why) | Refusal::Unexpected(why) => ArchiveError::Refused(why),
    })?;

    let notes = seen
        .keys()
        .filter(|path| path.starts_with(&format!("{NOTES_ENTRY}/")))
        .count();
    commit(home, scratch)?;
    Ok(ImportSummary {
        schema_version: inspection.schema_version,
        tickets: inspection.tickets,
        notes,
    })
}

/// The manifest and the entries unpacked agree exactly: every unpacked file is
/// listed with its size and digest, and every listed file was unpacked.
fn verify_against_manifest(
    manifest: &Manifest,
    seen: &BTreeMap<String, ManifestEntry>,
) -> Result<(), ArchiveError> {
    let mut listed: BTreeMap<&str, &ManifestEntry> = BTreeMap::new();
    for entry in &manifest.entries {
        if listed.insert(entry.path.as_str(), entry).is_some() {
            return Err(ArchiveError::Damaged(format!(
                "the manifest lists {} twice",
                entry.path
            )));
        }
    }
    for (path, actual) in seen {
        let Some(expected) = listed.get(path.as_str()) else {
            return Err(ArchiveError::Hostile {
                entry: path.clone(),
                reason: "not listed in the manifest".to_string(),
            });
        };
        if expected.size != actual.size {
            return Err(ArchiveError::Damaged(format!(
                "{path} is {} bytes, the manifest says {}",
                actual.size, expected.size
            )));
        }
        if !expected.sha256.eq_ignore_ascii_case(&actual.sha256) {
            return Err(ArchiveError::Damaged(format!(
                "{path} does not match its digest"
            )));
        }
    }
    for path in listed.keys() {
        if !seen.contains_key(*path) {
            return Err(ArchiveError::Damaged(format!(
                "{path} is listed in the manifest but missing from the archive"
            )));
        }
    }
    Ok(())
}

/// Two renames inside the home: the notes first, the store last, so the store's
/// arrival is the commit. A crash between them leaves notes and no store, which
/// the next import refuses as non-empty by name (`docs/store.md`).
fn commit(home: &Path, scratch: &Path) -> Result<(), ArchiveError> {
    // The home was empty when the import began; a `tandem serve` or another
    // import may have made a store since. Look again right before the moves.
    refuse_if_not_empty(home)?;
    let notes_from = scratch.join(NOTES_ENTRY);
    let notes_to = home.join(NOTES_ENTRY);
    let moved_notes = notes_from.is_dir();
    if moved_notes {
        // An empty `bikeshed/` is allowed to be there; a rename onto a
        // directory is not portable.
        if notes_to.is_dir() {
            fs::remove_dir(&notes_to).map_err(io_error("replace the empty bikeshed folder"))?;
        }
        fs::rename(&notes_from, &notes_to).map_err(io_error("move the notes into the home"))?;
    }
    // A hard link fails when the name exists, where a rename would replace a
    // store that appeared after the check: no merge, no force.
    let store_to = home.join(STORE_ENTRY);
    let outcome = match fs::hard_link(scratch.join(STORE_ENTRY), &store_to) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Err(refuse_if_not_empty(home).err().unwrap_or_else(|| {
                ArchiveError::Io(format!(
                    "Cannot move the store into the home: {} appeared during the import.",
                    store_to.display()
                ))
            }))
        }
        Err(error) => Err(io_error("move the store into the home")(error)),
    };
    if outcome.is_err() && moved_notes {
        let _ = fs::rename(&notes_to, &notes_from);
    }
    outcome?;
    // Both renames are durable only once the directory entries are synced.
    store::fsync_directory(home)?;
    Ok(())
}

// MARK: - Entry paths

/// Where an entry may land.
#[derive(Debug)]
enum Slot {
    Manifest,
    /// A file: `tandem.db` or `bikeshed/<path>`.
    File(String),
    /// A folder: `bikeshed` or `bikeshed/<path>`.
    Directory(String),
}

/// Decide whether an entry name is one of the three allowed places, or say why
/// not. Refused: an absolute path, an empty, `.` or `..` component, a backslash,
/// a NUL, a drive-like first component, anything outside `manifest.json`,
/// `tandem.db` and `bikeshed/`.
fn classify(raw: &[u8], is_dir: bool) -> Result<Slot, String> {
    let name = std::str::from_utf8(raw).map_err(|_| "a name that is not UTF-8".to_string())?;
    if name.contains('\0') {
        return Err("a NUL in the name".into());
    }
    if name.contains('\\') {
        return Err("a backslash in the name".into());
    }
    if name.starts_with('/') {
        return Err("an absolute path".into());
    }
    let name = if is_dir {
        name.strip_suffix('/').unwrap_or(name)
    } else {
        name
    };
    if name.is_empty() {
        return Err("an empty name".into());
    }
    let parts: Vec<&str> = name.split('/').collect();
    for part in &parts {
        match *part {
            "" => return Err("an empty path component".into()),
            "." => return Err("a `.` component".into()),
            ".." => return Err("a `..` component".into()),
            _ => {}
        }
    }
    if parts[0].contains(':') {
        return Err("a drive-like first component".into());
    }
    match (parts[0], parts.len(), is_dir) {
        (MANIFEST_ENTRY, 1, false) => Ok(Slot::Manifest),
        (STORE_ENTRY, 1, false) => Ok(Slot::File(name.to_string())),
        (NOTES_ENTRY, 1, true) => Ok(Slot::Directory(name.to_string())),
        (NOTES_ENTRY, n, true) if n > 1 => Ok(Slot::Directory(name.to_string())),
        (NOTES_ENTRY, n, false) if n > 1 => Ok(Slot::File(name.to_string())),
        _ => Err("a path outside manifest.json, tandem.db and bikeshed/".into()),
    }
}

// MARK: - Plumbing

fn damaged(error: io::Error) -> ArchiveError {
    ArchiveError::Damaged(match error.kind() {
        io::ErrorKind::UnexpectedEof => "it ends early".to_string(),
        _ => format!("it does not read as a gzip tar: {error}"),
    })
}

fn io_error(doing: &str) -> impl FnOnce(io::Error) -> ArchiveError {
    let doing = doing.to_string();
    move |error| ArchiveError::Io(format!("Cannot {doing}: {error}."))
}

/// An entry name for a message: lossy, with control characters escaped.
fn display(raw: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(raw))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unique() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{}-{nanos}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn make_private_dir(path: &Path) -> io::Result<()> {
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

fn make_private_dir_all(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

fn create_private_file(path: &Path) -> io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

/// A writer that counts what passes through it.
struct Counting<W> {
    inner: W,
    count: u64,
}

impl<W> Counting<W> {
    fn new(inner: W) -> Self {
        Self { inner, count: 0 }
    }
}

impl<W: Write> Write for Counting<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.count += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// A reader that hashes and counts what passes through it.
struct Hashing<R> {
    inner: R,
    hasher: Sha256,
    count: u64,
}

impl<R> Hashing<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            count: 0,
        }
    }
}

impl<R: Read> Read for Hashing<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.hasher.update(&buffer[..read]);
        self.count += read as u64;
        Ok(read)
    }
}

#[cfg(test)]
mod tests;
