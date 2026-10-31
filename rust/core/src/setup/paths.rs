//! The paths the install writes into, resolved from the run's environment.
//!
//! Ported from `SetupInstall.Context`: the face and its manifest sit in the
//! Tandem home, the host paths under the agent host's config root. Nothing here
//! names a path on one machine: `HOME`, `XDG_CONFIG_HOME` and the Tandem home
//! are the only inputs, so a test injects a throwaway tree and the shipped
//! binary reads the human's own.
//!
//! A path on the wire is `/`-separated on every platform
//! ([`crate::wire::slashed_path`]); the filesystem keeps the host's spelling.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// The environment a run reads: the two roots and the four flags ADR 0034 names.
pub type Environment = HashMap<String, String>;

/// Where one run's install lives.
#[derive(Debug, Clone)]
pub struct Context {
    home: PathBuf,
    environment: Environment,
}

impl Context {
    pub fn new(home: &Path, environment: &Environment) -> Self {
        Self {
            home: home.to_path_buf(),
            environment: environment.clone(),
        }
    }

    /// The Tandem home the face and the manifest live in.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// `<TandemHome>/agent/`: the one installed copy (ADR 0034).
    pub fn face(&self) -> PathBuf {
        self.home.join("agent")
    }

    /// `<TandemHome>/agent-install.json`, beside the face and never inside it.
    pub fn manifest_url(&self) -> PathBuf {
        self.home.join("agent-install.json")
    }

    /// The human's home: `HOME`, then `USERPROFILE`, then the process's own.
    pub fn user_home(&self) -> PathBuf {
        for name in ["HOME", "USERPROFILE"] {
            if let Some(value) = self.environment.get(name).filter(|value| !value.is_empty()) {
                return PathBuf::from(value);
            }
        }
        crate::store::user_home().unwrap_or_default()
    }

    /// `XDG_CONFIG_HOME`, else `~/.config`.
    pub fn config_home(&self) -> PathBuf {
        match self
            .environment
            .get("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
        {
            Some(value) => PathBuf::from(value),
            None => self.user_home().join(".config"),
        }
    }

    pub fn opencode_skills(&self) -> PathBuf {
        self.config_home().join("opencode").join("skills")
    }

    pub fn opencode_agents(&self) -> PathBuf {
        self.config_home().join("opencode").join("agents")
    }

    pub fn opencode_config(&self) -> PathBuf {
        self.config_home().join("opencode").join("opencode.json")
    }

    pub fn opencode_agents_md(&self) -> PathBuf {
        self.config_home().join("opencode").join("AGENTS.md")
    }

    pub fn claude_skills(&self) -> PathBuf {
        self.user_home().join(".claude").join("skills")
    }

    pub fn claude_md(&self) -> PathBuf {
        self.user_home().join(".claude").join("CLAUDE.md")
    }

    pub fn agents_skills(&self) -> PathBuf {
        self.user_home().join(".agents").join("skills")
    }

    /// The directory one host reads Tandem's face from: the root a Settings
    /// surface shows beside the host's name. OpenCode's is its config
    /// directory (which holds `skills/`, `agents/` and `opencode.json`), so
    /// the one root serves every path the install writes into.
    pub fn host_root(&self, host: super::Host) -> PathBuf {
        match host {
            super::Host::Opencode => self.config_home().join("opencode"),
            super::Host::Claude => self.user_home().join(".claude"),
            super::Host::Agents => self.user_home().join(".agents"),
        }
    }

    /// The four host directories the scan reads and the writability probe walks.
    pub fn host_directories(&self) -> [PathBuf; 4] {
        [
            self.opencode_skills(),
            self.opencode_agents(),
            self.claude_skills(),
            self.agents_skills(),
        ]
    }
}

/// The wire's spelling of a path: `/` on every platform.
pub fn text(path: &Path) -> String {
    crate::wire::slashed_path(path)
}

/// `path` with `.` and `..` resolved lexically, never touching the filesystem.
///
/// A symlink's stored target can be relative and can climb; the install's own
/// rule is about the path it names, so the answer must not depend on what the
/// target currently resolves to.
pub fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    normalized
}

/// `target` as an absolute path from the link at `link`, without resolving it.
pub fn absolute_target(target: &Path, link: &Path) -> PathBuf {
    if target.is_absolute() {
        return normalize(target);
    }
    let parent = link.parent().unwrap_or_else(|| Path::new("/"));
    normalize(&parent.join(target))
}

/// Remove a symbolic link, whatever it points at.
///
/// Unix has one `unlink(2)` for a file link and a directory link alike. Windows
/// does not: a directory symlink is removed with `RemoveDirectoryW`, a file
/// symlink with `DeleteFileW`, and asking the wrong one fails with access
/// denied. The target's type decides, and a dead link's own attributes still
/// carry the directory bit.
pub fn remove_link(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        // The target's type picks the call. A link whose target is gone can
        // still read as the other type, so a failed call is the signal to try
        // the other one rather than to give up.
        let directory = std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
            || std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir());
        if directory {
            std::fs::remove_dir(path).or_else(|_| std::fs::remove_file(path))
        } else {
            std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path))
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(path)
    }
}

/// A path with its symlinks resolved when it exists, and its lexical spelling
/// when it does not. The two spellings of one directory must compare equal:
/// `fs::canonicalize` keeps macOS's `/private` prefix, so the face is resolved
/// the same way before an "inside the face" test.
pub fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| normalize(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_target_is_joined_and_normalized() {
        let link = Path::new("/home/u/.config/opencode/skills/alpha");
        assert_eq!(
            absolute_target(Path::new("../../../face/skills/alpha"), link),
            Path::new("/home/u/face/skills/alpha")
        );
        assert_eq!(
            absolute_target(Path::new("/abs/target"), link),
            Path::new("/abs/target")
        );
    }

    #[test]
    fn the_config_root_is_xdg_then_dot_config() {
        let mut environment = Environment::new();
        environment.insert("HOME".into(), "/home/u".into());
        let context = Context::new(Path::new("/tandem"), &environment);
        assert_eq!(context.config_home(), Path::new("/home/u/.config"));
        assert_eq!(
            context.opencode_skills(),
            Path::new("/home/u/.config/opencode/skills")
        );

        environment.insert("XDG_CONFIG_HOME".into(), "/xdg".into());
        let context = Context::new(Path::new("/tandem"), &environment);
        assert_eq!(
            context.opencode_config(),
            Path::new("/xdg/opencode/opencode.json")
        );
    }
}
