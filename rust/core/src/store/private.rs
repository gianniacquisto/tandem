//! Owner-only creation of the files and folders Tandem keeps in its home.
//!
//! The home holds every ticket, note, member name and credential digest, so a
//! default umask of 022 must not make it readable by other local users. Each
//! creation site asks for its mode explicitly (0700 for a folder, 0600 for a
//! file) instead of a process-wide umask, which would also tighten files other
//! code writes outside the home. On Windows nothing changes.

use std::fs;
use std::io;
use std::path::Path;

/// Create `path` empty and owner-only (0600 on unix) with `create_new`. An
/// existing file is left as it is, so an older home is never changed quietly.
pub(crate) fn create_private_file(path: &Path) -> io::Result<()> {
    match create_new_private_file(path) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        other => other,
    }
}

/// Like [`create_private_file`] but an existing file is an error: the export
/// temp must be a fresh file.
pub(crate) fn create_new_private_file(path: &Path) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path).map(|_| ())
}

/// `create_dir_all` whose new folders are 0700 on unix. A folder that exists is
/// left alone.
pub(crate) fn create_dir_all_private(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

/// Write `bytes` to an owner-only file at `path`, truncating one that exists:
/// the notes' temp, created 0600 before its bytes land.
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    file.write_all(bytes)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn creates_private_whatever_the_umask() {
        let root = std::env::temp_dir().join(format!("tandem-private-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        create_dir_all_private(&root.join("a/b")).unwrap();
        assert_eq!(mode(&root.join("a")), 0o700);
        assert_eq!(mode(&root.join("a/b")), 0o700);
        create_private_file(&root.join("f")).unwrap();
        create_private_file(&root.join("f")).unwrap();
        assert_eq!(mode(&root.join("f")), 0o600);
        write_private(&root.join("g"), b"x").unwrap();
        assert_eq!(mode(&root.join("g")), 0o600);
        let _ = fs::remove_dir_all(&root);
    }
}
