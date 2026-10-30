//! Who made a write.
//!
//! The port's store records the actor on every mutation row (docs/port.md,
//! Store): the column is the truth, and no listing infers authorship from a
//! session row, a ticket's `origin` or the process's surroundings.
//!
//! The value comes from the place the harness already sets: `TANDEM_AGENT` is
//! the pass's name when an agent runs a verb, and unset (or blank) means the
//! human. One truth, so every verb takes the same variable and no verb grows an
//! `--actor` flag (PRT-4, Decided against).

use std::env;

/// The environment variable that names the actor of an agent-run write.
pub const ENVIRONMENT: &str = "TANDEM_AGENT";

/// Who is making a write. `human` is the app's face and an agent's unset
/// environment; a pass name (`builder`, `reviewer`, …) is the agent's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// The human, written as `human`.
    Human,
    /// An agent pass, written as the pass's name.
    Agent(String),
}

impl Actor {
    /// The pure precedence rule, so it can be tested without setting a
    /// process's environment: a non-blank value is the pass, anything else the
    /// human. Surrounding whitespace is not part of a name.
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim).filter(|name| !name.is_empty()) {
            Some(name) => Self::Agent(name.to_string()),
            None => Self::Human,
        }
    }

    /// The actor a process runs as: [`Actor::parse`] over `TANDEM_AGENT`.
    /// The CLI's write verbs call this once per run and hand it to
    /// [`Store::write`](super::Store::write); no verb grows an `--actor` flag.
    pub fn current() -> Self {
        Self::parse(env::var(ENVIRONMENT).ok().as_deref())
    }

    /// The word a mutation row records.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Human => "human",
            Self::Agent(name) => name,
        }
    }
}

impl std::fmt::Display for Actor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
