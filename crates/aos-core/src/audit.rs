//! Append only audit log.
//!
//! One line of JSON per call, including refusals. A refused call is the most interesting
//! line in the file, so anything that writes an entry only on success is wrong.

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{AgentId, Result, RiskTier};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Planned only. Nothing changed.
    DryRun,
    Allowed,
    Refused,
    /// The change landed but the post condition check did not confirm it.
    ///
    /// Deliberately not `Refused`. A failure reads as "nothing happened" and invites a
    /// retry that applies the change twice.
    AppliedButUnverified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unix seconds. Passed in rather than read from the clock so entries are testable.
    pub at: u64,
    pub agent: AgentId,
    pub action: String,
    pub tier: RiskTier,
    pub outcome: Outcome,
    /// Why, in one line. Required for a refusal so the log explains itself later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub trait AuditSink {
    fn record(&mut self, entry: &AuditEntry) -> Result<()>;
}

/// Writes one JSON object per line to a file that is only ever appended to.
pub struct JsonlSink {
    file: std::fs::File,
}

impl JsonlSink {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self { file })
    }
}

impl AuditSink for JsonlSink {
    fn record(&mut self, entry: &AuditEntry) -> Result<()> {
        let line = serde_json::to_string(entry)?;
        // One write call so two processes appending cannot interleave a partial line.
        self.file.write_all(format!("{line}\n").as_bytes())?;
        self.file.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(outcome: Outcome) -> AuditEntry {
        AuditEntry {
            at: 1_754_000_000,
            agent: AgentId::new("morning-brief").unwrap(),
            action: "agent.start".into(),
            tier: RiskTier::System,
            outcome,
            reason: None,
        }
    }

    #[test]
    fn entries_append_one_line_each() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");

        let mut sink = JsonlSink::open(&path).unwrap();
        sink.record(&entry(Outcome::Refused)).unwrap();
        sink.record(&entry(Outcome::Allowed)).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            serde_json::from_str::<AuditEntry>(line).unwrap();
        }
    }

    /// Reopening must not truncate. An audit log that loses history on restart is worse than
    /// no audit log, because it looks complete.
    #[test]
    fn reopening_appends_rather_than_truncates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");

        JsonlSink::open(&path)
            .unwrap()
            .record(&entry(Outcome::Allowed))
            .unwrap();
        JsonlSink::open(&path)
            .unwrap()
            .record(&entry(Outcome::Allowed))
            .unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 2);
    }
}
