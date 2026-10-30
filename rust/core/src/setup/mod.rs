//! The agent face's one writer (ADR 0034), ported from `SetupInstall.swift`.
//!
//! The rules, in one place because the three verbs must not disagree:
//!
//! - **One installed copy.** `<TandemHome>/agent/` is a real directory, replaced
//!   wholesale when the bundled `VERSION` differs; the host paths are symlinks
//!   into it, so an upgrade is one directory replace and no re-linking.
//! - **A link is somebody's decision.** A path that resolves into the face is the
//!   install's and stays; a dead link is replaced (a dead link is not a
//!   decision); a link elsewhere, a checkout included, is left and reported
//!   unless `--force`; a real file is never touched, `--force` included.
//! - **The manifest outlives the replace.** `<TandemHome>/agent-install.json`
//!   sits beside `agent/`, never inside it, and records the links, the host
//!   instruction edits and the hosts a run targeted.
//! - **One producer of warnings.** [`state`] is the only function that builds a
//!   [`Warning`]; `setup status`, `setup install` (after it writes) and
//!   `doctor`'s `agent-face` row all call it.
//!
//! The report types are the wire's: the Swift binary encodes the domain structs
//! directly, so the port keeps one definition and [`crate::wire`] points at it.

mod config;
mod face;
mod install;
mod manifest;
mod paths;
mod state;

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use config::{InstructionEdit, apply, instruction_edit};
pub use face::{install_face, partial_detail, prompts, skills, version_text};
pub use install::{install, status, status_with, sync_link, uninstall};
pub use manifest::{Manifest, manifest, write_manifest};
pub use paths::{Context, Environment, absolute_target, normalize, resolved, text};
pub use state::{
    ExpectedLink, classify, expected_links, is_inside_face, is_inside_face_lexical, is_tandems,
    ordered, state,
};

/// A program that reads the face. `opencode` is the default; the others are
/// written only when named, because each is a directory the human may not use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Host {
    Opencode,
    Claude,
    Agents,
}

impl Host {
    pub const ALL: [Host; 3] = [Host::Opencode, Host::Claude, Host::Agents];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Opencode => "opencode",
            Self::Claude => "claude",
            Self::Agents => "agents",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "opencode" => Some(Self::Opencode),
            "claude" => Some(Self::Claude),
            "agents" => Some(Self::Agents),
            _ => None,
        }
    }
}

/// `opencode` is the default; a comma-separated list names more than one.
pub fn hosts(raw: Option<&str>) -> std::result::Result<Vec<Host>, SetupError> {
    let Some(raw) = raw.filter(|value| !value.is_empty()) else {
        return Ok(vec![Host::Opencode]);
    };
    let mut result: Vec<Host> = Vec::new();
    for name in raw.split(',') {
        let trimmed = name.trim();
        let host =
            Host::parse(trimmed).ok_or_else(|| SetupError::UnknownHost(trimmed.to_string()))?;
        if !result.contains(&host) {
            result.push(host);
        }
    }
    Ok(if result.is_empty() {
        vec![Host::Opencode]
    } else {
        result
    })
}

/// `setup uninstall` without `--host` is a full run; with one it narrows.
pub fn uninstall_hosts(raw: Option<&str>) -> std::result::Result<Option<Vec<Host>>, SetupError> {
    match raw {
        None => Ok(None),
        Some(raw) => hosts(Some(raw)).map(Some),
    }
}

/// The setup verbs' failures: each one is a message an agent or human can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupError {
    /// No bundled face and no `TANDEM_AGENT_BUNDLE`: a dev build cannot invent one.
    NoBundle,
    /// The bundle exists but is not a face (no `VERSION`, no object to insert into).
    InvalidBundle(String),
    /// `--host` named a host that does not exist.
    UnknownHost(String),
}

impl fmt::Display for SetupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBundle => formatter.write_str(
                "no bundled agent face; run the CLI inside Tandem.app, or set \
                 TANDEM_AGENT_BUNDLE",
            ),
            Self::InvalidBundle(detail) => {
                write!(formatter, "the bundled agent face is not usable: {detail}")
            }
            Self::UnknownHost(name) => {
                write!(
                    formatter,
                    "unknown host {name}; name opencode, claude or agents"
                )
            }
        }
    }
}

impl std::error::Error for SetupError {}

/// The setup verbs also surface the filesystem's failures, one message each.
#[derive(Debug)]
pub enum Failure {
    Setup(SetupError),
    Io(String),
}

impl From<SetupError> for Failure {
    fn from(error: SetupError) -> Self {
        Self::Setup(error)
    }
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup(error) => error.fmt(formatter),
            Self::Io(detail) => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for Failure {}

pub type Result<T> = std::result::Result<T, Failure>;

// MARK: - The warning model

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warn,
    Info,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
        }
    }
}

/// The fourteen codes. The raw strings are the stable contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Code {
    NoBundle,
    NoManifest,
    StaleFace,
    MissingVersion,
    PartialFace,
    DeadLink,
    OrphanLink,
    ForeignLink,
    CheckoutShadowsInstall,
    BlockedPath,
    DoubleLoad,
    HostDisabled,
    HalfInstalled,
    Unwritable,
}

impl Code {
    /// The pinned list: a code that moves here is a wire change.
    pub const ALL: [Code; 14] = [
        Code::NoBundle,
        Code::NoManifest,
        Code::StaleFace,
        Code::MissingVersion,
        Code::PartialFace,
        Code::DeadLink,
        Code::OrphanLink,
        Code::ForeignLink,
        Code::CheckoutShadowsInstall,
        Code::BlockedPath,
        Code::DoubleLoad,
        Code::HostDisabled,
        Code::HalfInstalled,
        Code::Unwritable,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NoBundle => "no-bundle",
            Self::NoManifest => "no-manifest",
            Self::StaleFace => "stale-face",
            Self::MissingVersion => "missing-version",
            Self::PartialFace => "partial-face",
            Self::DeadLink => "dead-link",
            Self::OrphanLink => "orphan-link",
            Self::ForeignLink => "foreign-link",
            Self::CheckoutShadowsInstall => "checkout-shadows-install",
            Self::BlockedPath => "blocked-path",
            Self::DoubleLoad => "double-load",
            Self::HostDisabled => "host-disabled",
            Self::HalfInstalled => "half-installed",
            Self::Unwritable => "unwritable",
        }
    }
}

/// One warning. `path` is `nil` for a face-level code; the ordering rule and the
/// one-per-path rule need it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    pub code: Code,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub detail: String,
    pub hint: String,
}

impl Warning {
    pub fn new(
        code: Code,
        severity: Severity,
        path: Option<String>,
        detail: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity,
            path,
            detail: detail.into(),
            hint: hint.into(),
        }
    }
}

// MARK: - State and reports

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HostState {
    pub host: String,
    /// `installed`, `checkout`, `real-file` or `absent`.
    pub state: String,
    pub links: usize,
    pub expected_links: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct State {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundled_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
    pub hosts: Vec<HostState>,
    pub warnings: Vec<Warning>,
}

impl State {
    /// Whether a warning of `warn` or `error` stands: `doctor`'s row and the
    /// three verbs' exit codes all read this one rule.
    pub fn has_trouble(&self) -> bool {
        self.warnings
            .iter()
            .any(|warning| warning.severity != Severity::Info)
    }

    pub fn has_error(&self) -> bool {
        self.warnings
            .iter()
            .any(|warning| warning.severity == Severity::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PathAction {
    pub path: String,
    /// `linked`, `ok`, `replaced`, `skip` on install; `removed`, `left`,
    /// `absent` on uninstall.
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl PathAction {
    pub fn new(path: String, action: &str) -> Self {
        Self {
            path,
            action: action.to_string(),
            reason: None,
        }
    }

    pub fn with_reason(path: String, action: &str, reason: impl Into<String>) -> Self {
        Self {
            path,
            action: action.to_string(),
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub verb: String,
    /// `used` or `absent`.
    pub manifest: String,
    pub paths: Vec<PathAction>,
    pub state: State,
    /// Present only on an uninstall that found no manifest: the host links
    /// could not be attributed, and the report says so rather than guessing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unattributed: Option<bool>,
}

// MARK: - Where the bundle is

/// The agent face inside a `.app` this executable sits in: the candidate path,
/// whether or not it exists. `None` outside a release build in a bundle.
pub fn app_candidate(executable_path: &Path, is_debug_build: bool) -> Option<std::path::PathBuf> {
    if is_debug_build {
        return None;
    }
    let resolved_path = resolved(executable_path);
    let components: Vec<_> = resolved_path.components().collect();
    let index = components
        .iter()
        .position(|component| component.as_os_str().to_string_lossy().ends_with(".app"))?;
    let mut app = std::path::PathBuf::new();
    for component in &components[..=index] {
        app.push(component.as_os_str());
    }
    Some(app.join("Contents/Resources/agent"))
}

/// The bundled face, resolved in one place: `TANDEM_AGENT_BUNDLE` (the test
/// seam, the shape `TANDEM_HOME` already has), else the `.app` this executable
/// runs from, else none. A dev build cannot invent a bundle.
pub fn bundle(
    environment: &Environment,
    executable_path: &Path,
    is_debug_build: bool,
) -> Option<std::path::PathBuf> {
    if let Some(override_path) = environment
        .get("TANDEM_AGENT_BUNDLE")
        .filter(|value| !value.is_empty())
    {
        let expanded = expand_tilde(override_path, environment);
        return is_directory(&expanded).then_some(expanded);
    }
    let candidate = app_candidate(executable_path, is_debug_build)?;
    is_directory(&candidate).then_some(candidate)
}

/// This process's bundle: the resolver with this executable and build mode, so
/// the CLI and `doctor` need not read the two internal values themselves.
pub fn bundled_face(environment: &Environment) -> Option<std::path::PathBuf> {
    let executable = std::env::current_exe().unwrap_or_default();
    bundle(environment, &executable, cfg!(debug_assertions))
}

fn expand_tilde(raw: &str, environment: &Environment) -> std::path::PathBuf {
    if raw == "~" || raw.starts_with("~/") {
        let home = environment
            .get("HOME")
            .filter(|value| !value.is_empty())
            .or_else(|| {
                environment
                    .get("USERPROFILE")
                    .filter(|value| !value.is_empty())
            });
        if let Some(home) = home {
            let rest = raw.trim_start_matches('~').trim_start_matches('/');
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(raw)
}

pub(crate) fn is_directory(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
}
