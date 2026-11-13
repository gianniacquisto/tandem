//! The environment a run reads, as one injectable value.
//!
//! The home resolves from `TANDEM_HOME`, the MCP call's session pointer rides
//! beside it, and a test names a throwaway tree instead of the process's own.
//! The core reads no other source, so a value that is not UTF-8 is not a name
//! the domain knows and is dropped.

use std::collections::HashMap;

/// One run's environment: the names the core reads, and nothing else.
pub type Environment = HashMap<String, String>;

/// This process's environment, as [`Environment`].
pub fn process() -> Environment {
    std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}
