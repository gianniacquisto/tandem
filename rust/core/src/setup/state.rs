//! The face's state and the one producer of warnings, ported from
//! `SetupInstall.state`.
//!
//! `setup status`, `setup install` (after it writes) and `doctor`'s `agent-face`
//! row all read this function, so the three cannot disagree.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::config::{carries, instruction_edit};
use super::face::{partial_detail, prompts, skills, version_text};
use super::manifest::manifest;
use super::paths::{absolute_target, resolved, text};
use super::{
    Code, Context, Environment, Host, HostState, Severity, State, Warning, app_candidate,
    is_directory,
};

/// One host path the install expects, and the face path it should point at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedLink {
    /// The host path, for the filesystem.
    pub path: PathBuf,
    /// The host path in the wire's spelling.
    pub path_text: String,
    /// The face path the link names.
    pub target: String,
}

/// The state, with this process's executable and build mode.
pub fn current_state(home: &Path, environment: &Environment, bundle: Option<&Path>) -> State {
    let executable = std::env::current_exe().unwrap_or_default();
    state(
        home,
        environment,
        bundle,
        &executable,
        cfg!(debug_assertions),
    )
}

/// The state: the bundled and installed versions, each host's state and the
/// warnings, in the pinned order.
pub fn state(
    home: &Path,
    environment: &Environment,
    bundle: Option<&Path>,
    executable_path: &Path,
    is_debug_build: bool,
) -> State {
    let context = Context::new(home, environment);
    let bundle = bundle.filter(|path| is_directory(path));
    let bundled_version = bundle.and_then(version_text);
    let face = context.face();
    let face_exists = is_directory(&face);
    let installed_version = face_exists.then(|| version_text(&face)).flatten();
    let manifest = manifest(home);
    let recorded: HashSet<String> = manifest
        .as_ref()
        .map(|manifest| {
            manifest
                .links
                .iter()
                .map(|link| link.path.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut warnings: Vec<Warning> = Vec::new();

    if bundle.is_none() {
        let candidate = app_candidate(executable_path, is_debug_build);
        warnings.push(Warning::new(
            Code::NoBundle,
            if candidate.is_some() {
                Severity::Warn
            } else {
                Severity::Info
            },
            None,
            "no bundled agent face",
            "run inside Tandem.app, or set TANDEM_AGENT_BUNDLE",
        ));
    }

    if face_exists && manifest.is_none() {
        warnings.push(Warning::new(
            Code::NoManifest,
            Severity::Warn,
            Some(text(&face)),
            format!("{} exists with no agent-install.json", text(&face)),
            "tandem-cli setup install",
        ));
    }

    if face_exists {
        if installed_version.is_none() {
            warnings.push(Warning::new(
                Code::MissingVersion,
                Severity::Warn,
                Some(text(&face.join("VERSION"))),
                format!("{}/VERSION is absent", text(&face)),
                "tandem-cli setup install",
            ));
        } else if let (Some(bundled), Some(installed)) = (&bundled_version, &installed_version) {
            if bundled != installed {
                warnings.push(Warning::new(
                    Code::StaleFace,
                    Severity::Warn,
                    Some(text(&face)),
                    format!("installed {installed}, bundle {bundled}"),
                    "tandem-cli setup install",
                ));
            } else if let Some(bundle) = bundle
                && let Some(detail) = partial_detail(bundle, &face)
            {
                warnings.push(Warning::new(
                    Code::PartialFace,
                    Severity::Warn,
                    Some(text(&face)),
                    detail,
                    "tandem-cli setup install",
                ));
            }
        }
    }

    let targeted: HashSet<String> = manifest
        .as_ref()
        .map(|manifest| manifest.hosts.iter().cloned().collect())
        .unwrap_or_default();
    let mut expected: Vec<(Host, Vec<ExpectedLink>)> = Vec::new();
    for host in Host::ALL {
        let links = bundle
            .map(|bundle| expected_links(host, bundle, &context))
            .unwrap_or_default();
        expected.push((host, links));
    }
    let expected_links_for = |host: Host| -> Vec<ExpectedLink> {
        expected
            .iter()
            .find(|(candidate, _)| *candidate == host)
            .map(|(_, links)| links.clone())
            .unwrap_or_default()
    };

    let mut candidates: HashSet<PathBuf> = expected
        .iter()
        .flat_map(|(_, links)| links.iter().map(|link| link.path.clone()))
        .collect();
    for directory in context.host_directories() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            candidates.insert(entry.path());
        }
    }
    let mut candidates: Vec<PathBuf> = candidates.into_iter().collect();
    candidates.sort_by_key(|path| text(path));

    let current = match (&installed_version, &bundled_version) {
        (Some(installed), Some(bundled)) if installed == bundled => Some(installed.clone()),
        _ => None,
    };

    for path in candidates {
        if !is_tandems(&path, &context, bundle, &recorded) {
            continue;
        }
        if let Some(warning) = classify(&path, &context, &recorded, &targeted, current.as_deref()) {
            warnings.push(warning);
        }
    }

    // double-load: the old global AGENTS.md beside the instructions entry.
    let config_text = fs::read_to_string(context.opencode_config()).unwrap_or_default();
    let agents_md = text(&face.join("AGENTS.md"));
    if context.opencode_agents_md().exists() && config_text.contains(&agents_md) {
        let path = text(&context.opencode_agents_md());
        warnings.push(Warning::new(
            Code::DoubleLoad,
            Severity::Warn,
            Some(path.clone()),
            format!("{path} and the instructions entry both load {agents_md}"),
            format!("remove {path}; the instructions entry is the installed route"),
        ));
    }

    // host-disabled: a host is installed and OpenCode will not read it.
    let installed_hosts = installed_host_set(manifest.as_ref(), &expected, &context);
    for host in [Host::Claude, Host::Agents] {
        if !installed_hosts.contains(&host) {
            continue;
        }
        let names: [&str; 2] = if host == Host::Claude {
            [
                "OPENCODE_DISABLE_CLAUDE_CODE",
                "OPENCODE_DISABLE_CLAUDE_CODE_SKILLS",
            ]
        } else {
            [
                "OPENCODE_DISABLE_EXTERNAL_SKILLS",
                "OPENCODE_DISABLE_CLAUDE_CODE_SKILLS",
            ]
        };
        if let Some(name) = names
            .iter()
            .find(|name| truthy(environment.get(**name).map(String::as_str)))
        {
            warnings.push(Warning::new(
                Code::HostDisabled,
                Severity::Warn,
                None,
                format!("{} is installed, {name} is set", host.as_str()),
                format!(
                    "unset it, or tandem-cli setup uninstall --host {}",
                    host.as_str()
                ),
            ));
        }
    }

    // half-installed: the links without the instruction file, or the reverse.
    for host in [Host::Opencode, Host::Claude] {
        if !targeted.contains(host.as_str()) {
            continue;
        }
        let links = expected_links_for(host);
        if links.is_empty() {
            continue;
        }
        let present = links
            .iter()
            .filter(|link| resolves_into_face(&link.path, &context))
            .count();
        let instruction = match instruction_edit(host, &context) {
            Some(edit) => carries(&edit.file, edit.kind, &edit.entry),
            None => true,
        };
        if instruction && present == 0 {
            warnings.push(Warning::new(
                Code::HalfInstalled,
                Severity::Warn,
                None,
                format!("{} has its instruction file but no links", host.as_str()),
                format!("tandem-cli setup install --host {}", host.as_str()),
            ));
        } else if !instruction && present == links.len() {
            warnings.push(Warning::new(
                Code::HalfInstalled,
                Severity::Warn,
                None,
                format!("{} has its links but no instruction file", host.as_str()),
                format!("tandem-cli setup install --host {}", host.as_str()),
            ));
        }
    }

    // unwritable: probed on the nearest existing ancestor, so an absent home is
    // not an error.
    let mut probes = vec![home.to_path_buf()];
    probes.extend(context.host_directories());
    for path in probes {
        let probe = nearest_existing(&path);
        if !is_writable(&probe) {
            warnings.push(Warning::new(
                Code::Unwritable,
                Severity::Error,
                Some(text(&probe)),
                format!("{} is not writable", text(&probe)),
                "fix the permissions, or set TANDEM_HOME",
            ));
        }
    }

    let warnings = ordered(warnings);

    let mut hosts: Vec<HostState> = Vec::new();
    for host in Host::ALL {
        let links = expected_links_for(host);
        let present = links
            .iter()
            .filter(|link| resolves_into_face(&link.path, &context))
            .count();
        let state = if links.is_empty() {
            let recorded_links: Vec<&super::manifest::Link> = manifest
                .as_ref()
                .map(|manifest| {
                    manifest
                        .links
                        .iter()
                        .filter(|link| link.host == host.as_str())
                        .collect()
                })
                .unwrap_or_default();
            let live = recorded_links.iter().any(|link| {
                let target = fs::read_link(&link.path).ok().unwrap_or_default();
                let absolute = absolute_target(&target, Path::new(&link.path));
                resolved(&absolute).starts_with(resolved(&context.face()))
            });
            if live { "installed" } else { "absent" }
        } else if present == links.len() {
            "installed"
        } else if links
            .iter()
            .any(|link| is_foreign_link(&link.path, &context))
        {
            "checkout"
        } else if links.iter().any(|link| is_real_file(&link.path)) {
            "real-file"
        } else {
            "absent"
        };
        hosts.push(HostState {
            host: host.as_str().to_string(),
            state: state.to_string(),
            links: present,
            expected_links: links.len(),
        });
    }

    State {
        bundled_version,
        installed_version,
        hosts,
        warnings,
    }
}

/// The links one host gets: every skill, plus the prompts on OpenCode only.
pub fn expected_links(host: Host, bundle: &Path, context: &Context) -> Vec<ExpectedLink> {
    let face = context.face();
    let directory = match host {
        Host::Opencode => context.opencode_skills(),
        Host::Claude => context.claude_skills(),
        Host::Agents => context.agents_skills(),
    };
    let mut links: Vec<ExpectedLink> = skills(bundle)
        .into_iter()
        .map(|skill| {
            let path = directory.join(&skill);
            ExpectedLink {
                path_text: text(&path),
                path,
                target: text(&face.join("skills").join(&skill)),
            }
        })
        .collect();
    if host == Host::Opencode {
        for prompt in prompts(bundle) {
            let path = context.opencode_agents().join(&prompt);
            links.push(ExpectedLink {
                path_text: text(&path),
                path,
                target: text(&face.join("agents").join(&prompt)),
            });
        }
    }
    links
}

/// The first match wins, one record for the path (ADR 0034's table).
pub fn classify(
    path: &Path,
    context: &Context,
    recorded: &HashSet<String>,
    targeted: &HashSet<String>,
    current: Option<&str>,
) -> Option<Warning> {
    let metadata = fs::symlink_metadata(path).ok()?;
    let path_text = text(path);

    if !metadata.file_type().is_symlink() {
        let host = host_of(path, context)?;
        if !targeted.contains(host.as_str()) {
            return None;
        }
        return Some(Warning::new(
            Code::BlockedPath,
            Severity::Warn,
            Some(path_text.clone()),
            format!("{path_text} holds a real file"),
            "move it, then tandem-cli setup install",
        ));
    }

    let target = fs::read_link(path).ok().unwrap_or_default();
    let absolute = absolute_target(&target, path);
    let Some(resolved_target) = fs::canonicalize(&absolute).ok() else {
        return Some(Warning::new(
            Code::DeadLink,
            Severity::Warn,
            Some(path_text.clone()),
            format!("{path_text} -> {} (gone)", text(&absolute)),
            "tandem-cli setup install",
        ));
    };
    if is_inside_face(&resolved_target, context) {
        if recorded.contains(&path_text) {
            return None;
        }
        return Some(Warning::new(
            Code::OrphanLink,
            Severity::Warn,
            Some(path_text.clone()),
            format!("{path_text} is not in the manifest"),
            "tandem-cli setup install",
        ));
    }
    if let Some(current) = current {
        return Some(Warning::new(
            Code::CheckoutShadowsInstall,
            Severity::Warn,
            Some(path_text.clone()),
            format!(
                "{path_text} -> {}, beside a current install of {current}",
                text(&resolved_target)
            ),
            format!("remove the checkout link: rm {path_text}"),
        ));
    }
    Some(Warning::new(
        Code::ForeignLink,
        Severity::Info,
        Some(path_text.clone()),
        format!("{path_text} -> {}", text(&resolved_target)),
        "none: a checkout route is expected on a dev machine",
    ))
}

/// Whether `path` resolves into the installed face and the target exists.
pub fn resolves_into_face(path: &Path, context: &Context) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.file_type().is_symlink() {
        return false;
    }
    let target = fs::read_link(path).ok().unwrap_or_default();
    let absolute = absolute_target(&target, path);
    let Ok(resolved_target) = fs::canonicalize(&absolute) else {
        return false;
    };
    is_inside_face(&resolved_target, context)
}

fn is_foreign_link(path: &Path, context: &Context) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.file_type().is_symlink() {
        return false;
    }
    let target = fs::read_link(path).ok().unwrap_or_default();
    let absolute = absolute_target(&target, path);
    let Ok(resolved_target) = fs::canonicalize(&absolute) else {
        return false;
    };
    !is_inside_face(&resolved_target, context)
}

fn is_real_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.file_type().is_symlink())
}

/// Whether a resolved path is the face or sits inside it. Both sides are
/// resolved the same way, so macOS's `/private` prefix cannot split one
/// directory into two.
pub fn is_inside_face(path: &Path, context: &Context) -> bool {
    path.starts_with(resolved(&context.face()))
}

/// The same test against the face's own spelling, for a target that cannot be
/// canonicalized (a dead link). The install writes the face's lexical path, so
/// this is the comparison that recognizes its own dead links.
pub fn is_inside_face_lexical(path: &Path, context: &Context) -> bool {
    path.starts_with(super::paths::normalize(&context.face()))
}

/// A path is Tandem's, and therefore reported, only when its name is one the
/// bundle ships, the manifest records it, or it resolves into the face.
pub fn is_tandems(
    path: &Path,
    context: &Context,
    bundle: Option<&Path>,
    recorded: &HashSet<String>,
) -> bool {
    let path_text = text(path);
    if recorded.contains(&path_text) {
        return true;
    }
    if resolves_into_face(path, context) {
        return true;
    }
    if let Ok(target) = fs::read_link(path) {
        let absolute = absolute_target(&target, path);
        // The link may be dead, so it cannot be canonicalized; the face's own
        // spelling is what the install wrote, and the resolved spelling covers
        // a link written against the canonical path.
        if is_inside_face_lexical(&absolute, context) || is_inside_face(&absolute, context) {
            return true;
        }
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(bundle) = bundle
        && (skills(bundle).contains(&name) || prompts(bundle).contains(&name))
    {
        return true;
    }
    false
}

fn installed_host_set(
    manifest: Option<&super::manifest::Manifest>,
    expected: &[(Host, Vec<ExpectedLink>)],
    context: &Context,
) -> HashSet<Host> {
    let mut hosts: HashSet<Host> = manifest
        .map(|manifest| {
            manifest
                .hosts
                .iter()
                .filter_map(|name| Host::parse(name))
                .collect()
        })
        .unwrap_or_default();
    for (host, links) in expected {
        if links
            .iter()
            .any(|link| resolves_into_face(&link.path, context))
        {
            hosts.insert(*host);
        }
    }
    hosts
}

fn truthy(value: Option<&str>) -> bool {
    match value {
        Some(value) => value.eq_ignore_ascii_case("true") || value == "1",
        None => false,
    }
}

fn host_of(path: &Path, context: &Context) -> Option<Host> {
    if path.starts_with(context.opencode_skills()) || path.starts_with(context.opencode_agents()) {
        return Some(Host::Opencode);
    }
    if path.starts_with(context.claude_skills()) {
        return Some(Host::Claude);
    }
    if path.starts_with(context.agents_skills()) {
        return Some(Host::Agents);
    }
    None
}

/// Warnings deduplicated by code and path (or detail), then sorted by severity,
/// code and path. One path carries one code.
pub fn ordered(warnings: Vec<Warning>) -> Vec<Warning> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut unique: Vec<Warning> = Vec::new();
    for warning in warnings {
        let key = format!(
            "{}|{}",
            warning.code.as_str(),
            warning
                .path
                .clone()
                .unwrap_or_else(|| warning.detail.clone())
        );
        if seen.insert(key) {
            unique.push(warning);
        }
    }
    unique.sort_by(|left, right| {
        (
            left.severity,
            left.code.as_str(),
            left.path.clone().unwrap_or_default(),
        )
            .cmp(&(
                right.severity,
                right.code.as_str(),
                right.path.clone().unwrap_or_default(),
            ))
    });
    unique
}

/// The nearest path that exists, walking up from an absent one.
fn nearest_existing(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    while !current.exists() {
        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => break,
        }
    }
    current
}

/// Whether the process may write to `path`. Unix answers with `access(2)`, the
/// same probe the Swift core's `isWritableFile` makes; Windows has no such
/// portable probe here, so a path that exists is assumed writable and the
/// failure surfaces on the write itself.
fn is_writable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            return false;
        };
        // SAFETY: `c_path` is a valid NUL-terminated string for the call's
        // lifetime, and `access` does not retain it.
        unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
    }
    #[cfg(not(unix))]
    {
        path.exists()
    }
}
