//! The installed copy and the bundle it comes from, ported from `SetupInstall`'s
//! face half: the `VERSION` marker, the skill and prompt sets, and the atomic
//! replace.

use std::fs;
use std::path::{Path, PathBuf};

use super::Result;

/// The `VERSION` marker's text, trimmed; `None` when the marker is absent or
/// empty. This is the one version truth (ADR 0034).
pub fn version_text(directory: &Path) -> Option<String> {
    let text = fs::read_to_string(directory.join("VERSION")).ok()?;
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The skill directories a face carries, sorted by name.
pub fn skills(directory: &Path) -> Vec<String> {
    entries(&directory.join("skills"))
        .into_iter()
        .filter(|(name, path)| path.is_dir() && !name.is_empty())
        .map(|(name, _)| name)
        .collect()
}

/// The prompt files a face carries, sorted by name.
pub fn prompts(directory: &Path) -> Vec<String> {
    entries(&directory.join("agents"))
        .into_iter()
        .filter(|(name, path)| name.ends_with(".md") && path.is_file())
        .map(|(name, _)| name)
        .collect()
}

fn entries(directory: &Path) -> Vec<(String, PathBuf)> {
    let Ok(read) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = read
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (name, entry.path())
        })
        .collect();
    found.sort_by(|left, right| left.0.cmp(&right.0));
    found
}

/// What the installed copy is missing from the bundle, or carrying beyond it.
pub fn partial_detail(bundle: &Path, face: &Path) -> Option<String> {
    let face_skills = skills(face);
    let bundle_skills = skills(bundle);
    let face_prompts = prompts(face);
    let bundle_prompts = prompts(bundle);

    let missing = [
        missing_of(&bundle_skills, &face_skills, "skills/"),
        missing_of(&bundle_prompts, &face_prompts, "agents/"),
    ]
    .concat();
    let extra = [
        missing_of(&face_skills, &bundle_skills, "skills/"),
        missing_of(&face_prompts, &bundle_prompts, "agents/"),
    ]
    .concat();
    if missing.is_empty() && extra.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    if !missing.is_empty() {
        parts.push(format!("missing: {}", missing.join(", ")));
    }
    if !extra.is_empty() {
        parts.push(format!("extra: {}", extra.join(", ")));
    }
    Some(parts.join("; "))
}

fn missing_of(expected: &[String], found: &[String], prefix: &str) -> Vec<String> {
    let mut names: Vec<String> = expected
        .iter()
        .filter(|name| !found.contains(name))
        .map(|name| format!("{prefix}{name}"))
        .collect();
    names.sort();
    names
}

/// Materialise the face beside the old one, then rename it into place. A failure
/// removes the temporary tree and leaves the old copy intact.
pub fn install_face(bundle: &Path, face: &Path, version: &str) -> Result<()> {
    let home = face.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(home)?;

    let skill_names = skills(bundle);
    let prompt_names = prompts(bundle);
    let current = version_text(face);
    if current.as_deref() == Some(version)
        && skill_names == skills(face)
        && prompt_names == prompts(face)
        && face.join("AGENTS.md").exists()
    {
        return Ok(());
    }

    let temporary = home.join(format!("agent.new-{}", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        fs::create_dir_all(&temporary)?;
        for name in ["skills", "agents", "AGENTS.md"] {
            copy_tree(&bundle.join(name), &temporary.join(name))?;
        }
        fs::write(temporary.join("VERSION"), version.as_bytes())?;
        let old = home.join(format!("agent.old-{}", uuid::Uuid::new_v4()));
        let had_old = face.exists();
        if had_old {
            fs::rename(face, &old)?;
        }
        match fs::rename(&temporary, face) {
            Ok(()) => {
                if had_old {
                    let _ = remove_any(&old);
                }
                Ok(())
            }
            Err(error) => {
                if had_old {
                    let _ = fs::rename(&old, face);
                }
                Err(error.into())
            }
        }
    })();
    if result.is_err() {
        let _ = remove_any(&temporary);
    }
    result
}

/// Remove a file, a directory or a symlink, whichever is there. A path the
/// install owns can be any of the three: the face is a directory, but a human
/// may have left a link in its place.
pub fn remove_any(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// Copy a file or a directory tree. The bundle's face is plain content, so the
/// copy is the tree's bytes with no metadata to preserve.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let metadata = fs::metadata(from)?;
    if metadata.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_marker_is_trimmed_and_empty_is_none() {
        let root = std::env::temp_dir().join(format!("tandem-face-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert_eq!(version_text(&root), None);
        fs::write(root.join("VERSION"), "1.0\n").unwrap();
        assert_eq!(version_text(&root).as_deref(), Some("1.0"));
        fs::write(root.join("VERSION"), "  \n").unwrap();
        assert_eq!(version_text(&root), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_partial_face_names_what_is_missing_and_extra() {
        let root = std::env::temp_dir().join(format!("tandem-partial-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let bundle = root.join("bundle");
        let face = root.join("face");
        for directory in [&bundle, &face] {
            fs::create_dir_all(directory.join("skills/alpha")).unwrap();
            fs::create_dir_all(directory.join("agents")).unwrap();
        }
        fs::create_dir_all(bundle.join("skills/beta")).unwrap();
        fs::create_dir_all(face.join("skills/retired")).unwrap();
        assert_eq!(
            partial_detail(&bundle, &face).as_deref(),
            Some("missing: skills/beta; extra: skills/retired")
        );
        let _ = fs::remove_dir_all(&root);
    }
}
