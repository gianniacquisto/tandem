//! `setup install` and `setup uninstall`, ported from `SetupInstall`'s verbs.
//!
//! Install writes the face, then each host's links and instruction route, then
//! the manifest; a pre-probe refuses before anything is written when a root is
//! unwritable. Uninstall removes exactly what the manifest records, in order,
//! and never a path it did not record.

use std::fs;
use std::path::{Path, PathBuf};

use super::config::{apply, instruction_edit};
use super::face::{install_face, version_text};
use super::manifest::{Link, Manifest, manifest, record_link, write_manifest};
use super::paths::{absolute_target, resolved, text};
use super::state::{current_state, expected_links, is_inside_face, is_inside_face_lexical, state};
use super::{Context, Environment, Host, PathAction, Report, Result, SetupError, is_directory};

/// `setup install`: one installed copy, the host links, the instruction edits
/// and the manifest, in that order.
pub fn install(
    home: &Path,
    environment: &Environment,
    bundle: Option<&Path>,
    force: bool,
    hosts: &[Host],
) -> Result<Report> {
    let Some(bundle) = bundle.filter(|path| is_directory(path)) else {
        return Err(SetupError::NoBundle.into());
    };
    let Some(version) = version_text(bundle) else {
        return Err(
            SetupError::InvalidBundle(format!("{} has no VERSION marker", text(bundle))).into(),
        );
    };
    let context = Context::new(home, environment);

    // Probe before writing: an unwritable home or host directory leaves nothing
    // half-written, and the report carries the diagnosis.
    let preexisting = current_state(home, environment, Some(bundle));
    if preexisting.has_error() {
        return Ok(Report {
            verb: "install".to_string(),
            manifest: if manifest(home).is_none() {
                "absent"
            } else {
                "used"
            }
            .to_string(),
            paths: Vec::new(),
            state: preexisting,
            unattributed: None,
        });
    }

    install_face(bundle, &context.face(), &version)?;

    let mut record = manifest(home).unwrap_or_else(|| Manifest {
        version: version.clone(),
        ..Manifest::default()
    });
    record.version = version.clone();
    for host in hosts {
        if !record.hosts.iter().any(|name| name == host.as_str()) {
            record.hosts.push(host.as_str().to_string());
        }
    }

    let mut paths: Vec<PathAction> = Vec::new();
    for host in hosts {
        for expected in expected_links(*host, bundle, &context) {
            let action = sync_link(&expected.path, &expected.target, force, &context)?;
            let recordable = matches!(action.action.as_str(), "linked" | "ok" | "replaced");
            paths.push(action);
            if recordable {
                record_link(
                    &mut record,
                    Link::new(expected.path_text, expected.target, host.as_str()),
                );
            }
        }
        if let Some(edit) = instruction_edit(*host, &context) {
            paths.push(apply(&edit, &mut record)?);
        }
    }

    write_manifest(&record, &context.manifest_url())?;
    let after = current_state(home, environment, Some(bundle));
    Ok(Report {
        verb: "install".to_string(),
        manifest: "used".to_string(),
        paths,
        state: after,
        unattributed: None,
    })
}

/// `setup uninstall`: remove what the manifest records, then the face and the
/// manifest. A narrowed run takes only that host's records and rewrites the
/// manifest with the rest.
pub fn uninstall(home: &Path, environment: &Environment, hosts: Option<&[Host]>) -> Result<Report> {
    let context = Context::new(home, environment);
    let Some(manifest) = manifest(home) else {
        let mut actions: Vec<PathAction> = Vec::new();
        if hosts.is_none() {
            if context.face().exists() {
                super::face::remove_any(&context.face())?;
                actions.push(PathAction::new(text(&context.face()), "removed"));
            } else {
                actions.push(PathAction::new(text(&context.face()), "absent"));
            }
        }
        let after = current_state(home, environment, None);
        return Ok(Report {
            verb: "uninstall".to_string(),
            manifest: "absent".to_string(),
            paths: actions,
            state: after,
            unattributed: hosts.is_none().then_some(true),
        });
    };

    let narrowed: Option<std::collections::HashSet<&str>> =
        hosts.map(|hosts| hosts.iter().map(|host| host.as_str()).collect());
    let mut remaining = manifest.clone();
    let mut actions: Vec<PathAction> = Vec::new();

    for link in &manifest.links {
        if narrowed
            .as_ref()
            .is_some_and(|narrowed| !narrowed.contains(link.host.as_str()))
        {
            continue;
        }
        actions.push(remove_recorded_link(link, &context)?);
    }
    for edit in &manifest.edits {
        if narrowed
            .as_ref()
            .is_some_and(|narrowed| !narrowed.contains(edit.host.as_str()))
        {
            continue;
        }
        actions.push(remove_recorded_edit(edit)?);
    }
    remaining.links.retain(|link| {
        !narrowed
            .as_ref()
            .is_some_and(|narrowed| narrowed.contains(link.host.as_str()))
    });
    remaining.edits.retain(|edit| {
        !narrowed
            .as_ref()
            .is_some_and(|narrowed| narrowed.contains(edit.host.as_str()))
    });
    remaining.hosts.retain(|host| {
        !narrowed
            .as_ref()
            .is_some_and(|narrowed| narrowed.contains(host.as_str()))
    });

    if hosts.is_none() {
        if context.face().exists() {
            super::face::remove_any(&context.face())?;
            actions.push(PathAction::new(text(&context.face()), "removed"));
        } else {
            actions.push(PathAction::new(text(&context.face()), "absent"));
        }
        let _ = fs::remove_file(context.manifest_url());
    } else if remaining.links.is_empty() && remaining.edits.is_empty() && remaining.hosts.is_empty()
    {
        let _ = fs::remove_file(context.manifest_url());
    } else {
        write_manifest(&remaining, &context.manifest_url())?;
    }

    let after = current_state(home, environment, None);
    Ok(Report {
        verb: "uninstall".to_string(),
        manifest: "used".to_string(),
        paths: actions,
        state: after,
        unattributed: None,
    })
}

/// The link rule, in one function: absent is created, ours is `ok`, a dead link
/// is replaced, a foreign link is left unless forced, and a real file is never
/// touched.
pub fn sync_link(path: &Path, target: &str, force: bool, context: &Context) -> Result<PathAction> {
    let path_text = text(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let Ok(metadata) = fs::symlink_metadata(path) else {
        symlink(target, path)?;
        return Ok(PathAction::new(path_text, "linked"));
    };
    if !metadata.file_type().is_symlink() {
        return Ok(PathAction::with_reason(
            path_text,
            "skip",
            "a real file is there",
        ));
    }
    let raw = fs::read_link(path).unwrap_or_default();
    let absolute = absolute_target(&raw, path);
    if let Ok(resolved_target) = fs::canonicalize(&absolute) {
        if is_inside_face(&resolved_target, context) {
            return Ok(PathAction::new(path_text, "ok"));
        }
    } else {
        // A dead link is not a decision: replace it.
        fs::remove_file(path)?;
        symlink(target, path)?;
        return Ok(PathAction::new(path_text, "replaced"));
    }
    if !force {
        return Ok(PathAction::with_reason(
            path_text,
            "skip",
            format!("points elsewhere: {}", text(&resolved(&absolute))),
        ));
    }
    fs::remove_file(path)?;
    symlink(target, path)?;
    Ok(PathAction::new(path_text, "replaced"))
}

fn symlink(target: &str, path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, path)?;
    }
    #[cfg(windows)]
    {
        // A host path is a directory link for a skill and a file link for a
        // prompt; the target tells them apart.
        let target_path = Path::new(target);
        if target_path.is_dir() {
            std::os::windows::fs::symlink_dir(target_path, path)?;
        } else {
            std::os::windows::fs::symlink_file(target_path, path)?;
        }
    }
    Ok(())
}

fn remove_recorded_link(link: &Link, context: &Context) -> Result<PathAction> {
    let path = PathBuf::from(&link.path);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return Ok(PathAction::new(link.path.clone(), "absent"));
    };
    if !metadata.file_type().is_symlink() {
        return Ok(PathAction::with_reason(
            link.path.clone(),
            "left",
            "a real file is there",
        ));
    }
    let raw = fs::read_link(&path).unwrap_or_default();
    let absolute = absolute_target(&raw, &path);
    let resolved_target = resolved(&absolute);
    if !is_inside_face_lexical(&absolute, context) && !is_inside_face(&resolved_target, context) {
        return Ok(PathAction::with_reason(
            link.path.clone(),
            "left",
            format!("points elsewhere: {}", text(&resolved_target)),
        ));
    }
    fs::remove_file(&path)?;
    Ok(PathAction::new(link.path.clone(), "removed"))
}

fn remove_recorded_edit(edit: &super::manifest::Edit) -> Result<PathAction> {
    let path = Path::new(&edit.file);
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(PathAction::new(edit.file.clone(), "absent"));
    };
    let Some(start) = text.find(&edit.text) else {
        return Ok(PathAction::with_reason(
            edit.file.clone(),
            "left",
            "the recorded text is gone",
        ));
    };
    let remaining = format!("{}{}", &text[..start], &text[start + edit.text.len()..]);
    if remaining.trim().is_empty() {
        fs::remove_file(path)?;
    } else {
        fs::write(path, remaining.as_bytes())?;
    }
    Ok(PathAction::new(edit.file.clone(), "removed"))
}

/// `setup status` for this process's own bundle.
pub fn status(home: &Path, environment: &Environment) -> Report {
    let bundle = super::bundled_face(environment);
    let executable = std::env::current_exe().unwrap_or_default();
    status_with(
        home,
        environment,
        bundle.as_deref(),
        &executable,
        cfg!(debug_assertions),
    )
}

/// `setup status`: the state, framed as the same report the other two verbs
/// print, so one encoder serves all three.
pub fn status_with(
    home: &Path,
    environment: &Environment,
    bundle: Option<&Path>,
    executable_path: &Path,
    is_debug_build: bool,
) -> Report {
    Report {
        verb: "status".to_string(),
        manifest: if manifest(home).is_none() {
            "absent"
        } else {
            "used"
        }
        .to_string(),
        paths: Vec::new(),
        state: state(home, environment, bundle, executable_path, is_debug_build),
        unattributed: None,
    }
}
