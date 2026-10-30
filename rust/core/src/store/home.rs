//! Where a run keeps its state: the store, the bikeshed, the lock sidecar.
//!
//! The rule, carried from the Swift core (`TandemHome.swift`): a run reads and
//! writes exactly one home, and every path it derives comes from this value.
//! Nothing under this module touches `~/.tandem` unless the resolution says so.
//!
//! The real home belongs to the shipped app. Until cutover the Rust binary is
//! pointed at a fixture home (`docs/port.md`, How the work lands), which is why
//! `--home` exists at all and why the CLI's tests run with a sandbox `HOME`.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The one environment variable that overrides the home.
pub const ENVIRONMENT: &str = "TANDEM_HOME";

/// Why a run resolved where it did. Reported verbatim, one variant per branch
/// of the precedence rule, so a report cannot disagree with the resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum HomeSource {
    /// `--home` was given and wins outright.
    Flag,
    /// `TANDEM_HOME` was set non-empty.
    Environment,
    /// Neither: the user's `~/.tandem`.
    Default,
}

/// The directory a process keeps its state in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    directory: PathBuf,
    source: HomeSource,
}

impl Home {
    /// The pure rule, so it can be tested without setting a process's
    /// environment.
    ///
    /// Precedence: `--home`, then a non-empty `TANDEM_HOME`, then
    /// `<user home>/.tandem`. A leading `~` in either override is the user's
    /// home; anything else is taken from the working directory, so a relative
    /// `--home` means what the caller typed it in.
    pub fn resolve(flag: Option<&Path>, environment: Option<&str>, user_home: &Path) -> Self {
        if let Some(path) = flag {
            return Self::overridden(path, HomeSource::Flag, user_home);
        }
        match environment.filter(|value| !value.is_empty()) {
            Some(value) => Self::overridden(Path::new(value), HomeSource::Environment, user_home),
            None => Self {
                directory: user_home.join(".tandem"),
                source: HomeSource::Default,
            },
        }
    }

    fn overridden(path: &Path, source: HomeSource, user_home: &Path) -> Self {
        let expanded = expand_tilde(path, user_home);
        let directory = match std::path::absolute(&expanded) {
            Ok(absolute) => normalize(absolute),
            Err(_) => expanded,
        };
        Self { directory, source }
    }

    /// The directory itself, absolute.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Which branch of the rule produced [`Home::directory`].
    pub fn source(&self) -> HomeSource {
        self.source
    }

    /// Create the home if this is the first run to reach it.
    ///
    /// A run is a run in a home, so the run makes sure there is one. Nothing
    /// inside it is written here: the store and the bikeshed own their files.
    /// An existing home is left untouched, so a read never changes a mtime.
    pub fn ensure(&self) -> io::Result<()> {
        fs::create_dir_all(&self.directory)
    }
}

/// The user's home directory: `HOME`, then `USERPROFILE` on Windows.
pub fn user_home() -> io::Result<PathBuf> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .find_map(|name| env::var_os(name).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no HOME or USERPROFILE in the environment",
            )
        })
}

/// `~` and `~/x` as the user's home; every other path as it was written.
fn expand_tilde(path: &Path, user_home: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    if text == "~" {
        return user_home.to_path_buf();
    }
    match text.strip_prefix("~/") {
        Some(rest) => user_home.join(rest),
        None => path.to_path_buf(),
    }
}

/// Remove `.` and resolve `..` lexically, never touching the filesystem.
///
/// The home is printed by `doctor` and measured by the tests, so two spellings
/// of one directory must not read as two homes. Symlinks stay unresolved: the
/// rule describes a path, and resolving a link would make it depend on the
/// machine it runs on (`TandemHome.resolvedPath` in the Swift core is the app
/// bundle's test, not the home's).
fn normalize(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                // An absolute path cannot climb above its root, so a `..` the
                // root cannot absorb is dropped, not carried around.
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every path here is platform-shaped: a `/users/test` literal resolves to
    // `D:\users\test` on Windows and the assertions would compare two homes
    // that never met. The temp directory is absolute on all three runners.
    fn user_home() -> PathBuf {
        std::env::temp_dir().join("tandem-user-home")
    }

    fn absolute(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn the_flag_wins_over_the_environment() {
        let home = Home::resolve(
            Some(&absolute("flag-home")),
            Some(absolute("environment-home").to_str().unwrap()),
            &user_home(),
        );
        assert_eq!(home.directory(), absolute("flag-home"));
        assert_eq!(home.source(), HomeSource::Flag);
    }

    #[test]
    fn a_non_empty_environment_wins_over_the_default() {
        let environment = absolute("environment-home");
        let home = Home::resolve(None, Some(environment.to_str().unwrap()), &user_home());
        assert_eq!(home.directory(), environment);
        assert_eq!(home.source(), HomeSource::Environment);
    }

    #[test]
    fn an_empty_environment_is_no_override() {
        let home = Home::resolve(None, Some(""), &user_home());
        assert_eq!(home.directory(), user_home().join(".tandem"));
        assert_eq!(home.source(), HomeSource::Default);
    }

    #[test]
    fn the_default_is_dot_tandem_under_the_user_home() {
        let home = Home::resolve(None, None, &user_home());
        assert_eq!(home.directory(), user_home().join(".tandem"));
        assert_eq!(home.source(), HomeSource::Default);
    }

    #[test]
    fn a_leading_tilde_in_an_override_is_the_user_home() {
        let bare = Home::resolve(Some(Path::new("~")), None, &user_home());
        assert_eq!(bare.directory(), user_home());

        let nested = Home::resolve(Some(Path::new("~/fixtures/home")), None, &user_home());
        assert_eq!(
            nested.directory(),
            user_home().join("fixtures").join("home")
        );
    }

    #[test]
    fn a_relative_override_is_taken_from_the_working_directory() {
        let home = Home::resolve(Some(Path::new("Fixtures/port/Home")), None, &user_home());
        let expected = std::env::current_dir().unwrap().join("Fixtures/port/Home");
        assert_eq!(home.directory(), expected);
    }

    #[test]
    fn dot_and_dot_dot_are_resolved_lexically() {
        let home = Home::resolve(
            Some(Path::new("Fixtures/../Fixtures/./port/Home")),
            None,
            &user_home(),
        );
        let expected = std::env::current_dir()
            .unwrap()
            .join("Fixtures")
            .join("port")
            .join("Home");
        assert_eq!(home.directory(), expected);
    }
}
