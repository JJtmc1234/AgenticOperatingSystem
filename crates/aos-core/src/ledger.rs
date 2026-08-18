//! The append only log.

use std::io::Write;
use std::path::Path;

use crate::{AgentId, Event, Record, Result};

/// One JSON object per line, only ever appended to.
///
/// Text rather than SQLite on purpose. The log has to be readable with `cat` when the thing
/// that writes it is the thing that is broken.
pub struct Ledger {
    sink: Box<dyn std::io::Write>,
    next_seq: u64,
}

impl Ledger {
    /// Opens the log, reading it once to find where the sequence left off.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let next_seq = read(path)?.last().map_or(1, |r| r.seq + 1);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            sink: Box::new(file),
            next_seq,
        })
    }

    /// A ledger over any sink, starting the sequence at `next_seq`.
    ///
    /// This exists so a failing write can be reached on purpose. Every belief the runtime
    /// holds is a fold over the log, which makes "the log refused a write" the failure worth
    /// testing hardest, and it is also the one a real file will not reproduce on demand. A
    /// guard nobody can trigger is a guess, so the seam is part of the product rather than
    /// something bolted on beside it.
    pub fn to_sink(sink: Box<dyn std::io::Write>, next_seq: u64) -> Self {
        Self { sink, next_seq }
    }

    pub fn append(&mut self, at: u64, agent: AgentId, event: Event) -> Result<Record> {
        let record = Record {
            seq: self.next_seq,
            at,
            agent,
            event,
        };
        // One write call, so two writers appending cannot interleave a partial line.
        self.sink
            .write_all(format!("{}\n", serde_json::to_string(&record)?).as_bytes())?;
        self.sink.flush()?;
        self.next_seq += 1;
        Ok(record)
    }
}

/// Every record in order. A missing file is an empty log, not an error, because the first
/// boot has nothing to replay.
pub fn read(path: impl AsRef<Path>) -> Result<Vec<Record>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProcessHandle;

    fn started(pid: u32) -> Event {
        Event::Started {
            handle: ProcessHandle {
                pid,
                start_token: pid as u64 * 7,
            },
            program: "/usr/bin/sleep".into(),
        }
    }

    fn id(name: &str) -> AgentId {
        AgentId::new(name).unwrap()
    }

    #[test]
    fn sequence_continues_across_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let mut ledger = Ledger::open(&path).unwrap();
        assert_eq!(ledger.append(1, id("a"), started(10)).unwrap().seq, 1);
        assert_eq!(ledger.append(2, id("b"), started(11)).unwrap().seq, 2);

        // A restart must not reset the sequence, or two records share a number and the log
        // stops being an ordering.
        let mut reopened = Ledger::open(&path).unwrap();
        assert_eq!(reopened.append(3, id("c"), started(12)).unwrap().seq, 3);
        assert_eq!(read(&path).unwrap().len(), 3);
    }

    #[test]
    fn a_missing_log_replays_as_empty() {
        assert!(read("/tmp/aos-no-such-log.jsonl").unwrap().is_empty());
    }

    /// Blank lines are tolerated, because a log truncated by a full disk should still be
    /// mostly readable rather than entirely useless.
    #[test]
    fn blank_lines_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        Ledger::open(&path)
            .unwrap()
            .append(1, id("a"), started(10))
            .unwrap();
        std::fs::write(
            &path,
            format!("\n{}\n\n", std::fs::read_to_string(&path).unwrap().trim()),
        )
        .unwrap();

        assert_eq!(read(&path).unwrap().len(), 1);
    }
}
