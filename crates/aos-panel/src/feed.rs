//! The live half of the panel, read from the ledger rather than asked for.
//!
//! `aosd` serves one connection at a time on purpose: it owns process lifetimes, and two
//! requests racing to start the same agent leaves a stray process behind. A panel holding a
//! subscription open would therefore lock every other client out, including the cli, so the
//! panel does not subscribe. It reads the ledger.
//!
//! That is not a workaround. Phase 0r made the ledger the source of truth, and it is an append
//! only file with a monotonic sequence that never goes backwards. Tailing it gives the panel
//! exactly what the daemon knows, in the order the daemon decided it, without asking the daemon
//! for anything and without being able to disturb it. The file is opened read only.
//!
//! Only whole lines are consumed. `append` reaches the file as more than one write, so a reader
//! that moved its offset past a half written line would lose that record for good.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::Result;
use aos_core::Record;

/// Where a reader has got to, so it can carry on without rereading the whole file.
pub struct Feed {
    path: PathBuf,
    /// Bytes consumed. Only ever advanced past a newline.
    offset: u64,
    /// The highest sequence handed out, so a gap is noticed rather than absorbed.
    last_seq: u64,
}

/// What one read of the ledger produced.
#[derive(Debug, Default)]
pub struct Fresh {
    pub records: Vec<Record>,
    /// True when the file got shorter, which means it was replaced rather than appended to.
    /// Everything held from before is history and the caller should start again.
    pub restarted: bool,
}

impl Feed {
    /// Starts at the beginning, so the first read replays everything the daemon has recorded.
    pub fn new(run_dir: &Path) -> Self {
        Self {
            path: run_dir.join("events.jsonl"),
            offset: 0,
            last_seq: 0,
        }
    }

    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Everything appended since the last call.
    ///
    /// A ledger that has shrunk was replaced, not appended to, so the offset is reset and the
    /// caller is told. Carrying on from a stale offset would read from the middle of a record.
    pub fn read(&mut self) -> Result<Fresh> {
        let mut file = match std::fs::File::open(&self.path) {
            Ok(f) => f,
            // No daemon has ever run here. Not an error, just nothing yet.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Fresh::default()),
            Err(e) => return Err(e.into()),
        };

        let size = file.metadata()?.len();
        let mut out = Fresh::default();
        if size < self.offset {
            self.offset = 0;
            self.last_seq = 0;
            out.restarted = true;
        }
        if size == self.offset {
            return Ok(out);
        }

        file.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::new();
        file.take(size - self.offset).read_to_end(&mut buf)?;

        // Stop at the last newline. Anything after it is a record still being written.
        let end = match buf.iter().rposition(|b| *b == b'\n') {
            Some(i) => i + 1,
            None => return Ok(out),
        };

        for line in buf[..end].split(|b| *b == b'\n') {
            if line.is_empty() {
                continue;
            }
            // A line that will not parse is skipped rather than fatal, the same way the ledger
            // reader treats one. A log with one damaged line is still worth reading.
            if let Ok(record) = serde_json::from_slice::<Record>(line) {
                self.last_seq = self.last_seq.max(record.seq);
                out.records.push(record);
            }
        }
        self.offset += end as u64;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_core::{AgentId, Event, Record};

    fn record(seq: u64) -> Record {
        Record {
            seq,
            at: 1_700_000_000 + seq,
            agent: AgentId::new("morning-brief").unwrap(),
            event: Event::Exited { code: Some(0) },
        }
    }

    fn write(dir: &Path, records: &[Record]) {
        let text: String = records
            .iter()
            .map(|r| format!("{}\n", serde_json::to_string(r).unwrap()))
            .collect();
        std::fs::write(dir.join("events.jsonl"), text).unwrap();
    }

    fn append(dir: &Path, text: &str) {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("events.jsonl"))
            .unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn a_run_dir_with_no_ledger_yet_is_empty_rather_than_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut feed = Feed::new(dir.path());
        let fresh = feed.read().unwrap();
        assert!(fresh.records.is_empty());
        assert!(!fresh.restarted);
        assert_eq!(feed.last_seq(), 0);
    }

    #[test]
    fn the_first_read_replays_everything_and_the_next_returns_nothing() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &[record(1), record(2), record(3)]);

        let mut feed = Feed::new(dir.path());
        let first = feed.read().unwrap();
        assert_eq!(first.records.len(), 3);
        assert_eq!(feed.last_seq(), 3);

        let again = feed.read().unwrap();
        assert!(
            again.records.is_empty(),
            "nothing was appended, so nothing is new"
        );
        assert_eq!(feed.last_seq(), 3, "and the position did not move");
    }

    #[test]
    fn only_what_was_appended_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &[record(1)]);
        let mut feed = Feed::new(dir.path());
        feed.read().unwrap();

        append(
            dir.path(),
            &format!("{}\n", serde_json::to_string(&record(2)).unwrap()),
        );
        let fresh = feed.read().unwrap();
        assert_eq!(fresh.records.len(), 1, "only the new one");
        assert_eq!(fresh.records[0].seq, 2);
    }

    /// `append` reaches the file as more than one write, so a reader can arrive mid record.
    /// Consuming a half written line would move the offset past it and lose it for good.
    #[test]
    fn a_half_written_record_is_left_alone_until_it_is_whole() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &[record(1)]);
        let mut feed = Feed::new(dir.path());
        assert_eq!(feed.read().unwrap().records.len(), 1);

        let whole = serde_json::to_string(&record(2)).unwrap();
        let (head, tail) = whole.split_at(whole.len() / 2);
        append(dir.path(), head);
        assert!(
            feed.read().unwrap().records.is_empty(),
            "half a record is not a record"
        );

        append(dir.path(), &format!("{tail}\n"));
        let fresh = feed.read().unwrap();
        assert_eq!(fresh.records.len(), 1, "and it arrives once it is whole");
        assert_eq!(fresh.records[0].seq, 2);
    }

    /// A ledger that shrank was replaced. Carrying on from the old offset would start reading
    /// from the middle of a record, so the reader starts again and says so.
    #[test]
    fn a_replaced_ledger_is_noticed_rather_than_read_from_the_middle() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &[record(1), record(2), record(3), record(4)]);
        let mut feed = Feed::new(dir.path());
        assert_eq!(feed.read().unwrap().records.len(), 4);

        write(dir.path(), &[record(1)]);
        let fresh = feed.read().unwrap();
        assert!(
            fresh.restarted,
            "the caller has to be told its history is gone"
        );
        assert_eq!(fresh.records.len(), 1);
        assert_eq!(feed.last_seq(), 1, "and the sequence starts again with it");
    }

    #[test]
    fn a_damaged_line_costs_only_itself() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &[record(1)]);
        append(dir.path(), "{not json at all}\n");
        append(
            dir.path(),
            &format!("{}\n", serde_json::to_string(&record(3)).unwrap()),
        );

        let mut feed = Feed::new(dir.path());
        let fresh = feed.read().unwrap();
        assert_eq!(fresh.records.len(), 2, "the two good ones");
        assert_eq!(
            fresh.records[1].seq, 3,
            "including the one after the damage"
        );
    }

    #[test]
    fn sequences_only_ever_go_forwards() {
        let dir = tempfile::tempdir().unwrap();
        let mut feed = Feed::new(dir.path());
        let mut seen = Vec::new();
        for seq in 1..=5 {
            append(
                dir.path(),
                &format!("{}\n", serde_json::to_string(&record(seq)).unwrap()),
            );
            for r in feed.read().unwrap().records {
                seen.push(r.seq);
            }
        }
        assert_eq!(seen, vec![1, 2, 3, 4, 5], "in order, once each");
        assert_eq!(feed.last_seq(), 5);
    }
}
