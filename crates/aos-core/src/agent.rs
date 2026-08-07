//! What an agent is, before anything runs it.

use serde::{Deserialize, Serialize};

use crate::{Error, Result, RiskTier};

/// A validated agent name. Constructing one is the only way to get a name into the rest of
/// the system, so an unchecked string can never reach a file path or a log line.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AgentId(String);

impl AgentId {
    /// Accepts lowercase letters, digits and dashes only.
    ///
    /// The character set is narrow on purpose. Agent ids become filenames under the run
    /// directory, so anything that could mean "parent directory" or "separator" to a path
    /// is refused here rather than guarded at every use site.
    pub fn new(raw: impl Into<String>) -> Result<Self> {
        let raw = raw.into();
        let shaped = !raw.is_empty()
            && raw.len() <= 64
            && raw
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !raw.starts_with('-')
            && !raw.ends_with('-');

        if shaped {
            Ok(Self(raw))
        } else {
            Err(Error::BadAgentId(raw))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for AgentId {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<AgentId> for String {
    fn from(value: AgentId) -> Self {
        value.0
    }
}

/// The declaration of an agent. Data only, so it can be read from a file and checked
/// against policy before any process exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSpec {
    pub id: AgentId,
    /// Program to launch. Resolved against the allowlist, never through a shell.
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Highest tier this agent may reach. A call above it is refused.
    pub ceiling: RiskTier,
}

/// Where an agent is in its life. `Stopped` carries the exit code when one exists, which is
/// `None` for a process killed by a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "state")]
pub enum AgentState {
    Running { pid: u32 },
    Stopped { code: Option<i32> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names_are_accepted() {
        assert_eq!(
            AgentId::new("morning-brief").unwrap().as_str(),
            "morning-brief"
        );
        assert_eq!(AgentId::new("a1").unwrap().as_str(), "a1");
    }

    /// Harness invariant. An agent id becomes a filename, so every shape that could escape
    /// the run directory or confuse a path parser has to be refused at construction.
    #[test]
    fn path_shaped_names_are_refused() {
        let bad = [
            "",
            "..",
            "../etc",
            "a/b",
            "a\\b",
            "Agent",
            "a b",
            "a.b",
            "-lead",
            "trail-",
            "nul\0byte",
            "a:b",
        ];
        for name in bad {
            assert!(
                AgentId::new(name).is_err(),
                "{name:?} should not be a valid agent id"
            );
        }
    }

    #[test]
    fn overlong_names_are_refused() {
        assert!(AgentId::new("a".repeat(64)).is_ok());
        assert!(AgentId::new("a".repeat(65)).is_err());
    }

    /// Deserialising must run the same check as the constructor, otherwise a hand edited
    /// spec file is a way around it.
    #[test]
    fn deserialising_a_bad_id_fails() {
        assert!(serde_json::from_str::<AgentId>("\"../etc/passwd\"").is_err());
    }
}
