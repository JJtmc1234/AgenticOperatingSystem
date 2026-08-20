//! The append only log.

use std::io::Write;
use std::path::Path;

use crate::{AgentId, Event, Record, Result};

/// One JSON object per line, only ever appended to.
///
/// Text rather than SQLite on purpose. The log has to be readable with `cat` when the thing
/// that writes it is the thing that is broken.
pub struct Ledger {
    file: std::fs::File,
    next_seq: u64,
}

impl Ledger {
    /// Opens the log, reading it once to find where the sequence left off.
    ///
    /// Repairs an unterminated final line first. A daemon that refuses to boot over a half
    /// written line is worse than one that drops it: the agents from the previous run are
    /// still on the machine, and refusing means nothing adopts them, stops them or records
    /// them as lost, on this boot or any later one.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        repair_torn_tail(path)?;
        let next_seq = read(path)?.last().map_or(1, |r| r.seq + 1);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self { file, next_seq })
    }

    pub fn append(&mut self, at: u64, agent: AgentId, event: Event) -> Result<Record> {
        let record = Record {
            seq: self.next_seq,
            at,
            agent,
            event,
        };
        // One write call, so two writers appending cannot interleave a partial line.
        self.file
            .write_all(format!("{}\n", serde_json::to_string(&record)?).as_bytes())?;
        self.file.flush()?;
        self.next_seq += 1;
        Ok(record)
    }
}

/// Every record in order. A missing file is an empty log, not an error, because the first
/// boot has nothing to replay.
///
/// A torn last line is tolerated, and only a torn last line. If the file does not end in a
/// newline then the final line was still being written when the machine stopped, so it never
/// described anything that happened and dropping it loses nothing. A bad line anywhere else is
/// still an error, because that is corruption of a record that was completed once, and quietly
/// skipping it would let the log disagree with the world without saying so. See bug 7.
pub fn read(path: impl AsRef<Path>) -> Result<Vec<Record>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    parse(&text)
}

/// Splits the log into records, forgiving an unterminated final line.
fn parse(text: &str) -> Result<Vec<Record>> {
    let terminated = text.is_empty() || text.ends_with('\n');
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();

    let mut records = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        match serde_json::from_str(line) {
            Ok(record) => records.push(record),
            // Unparsable and unterminated and last. All three, or it is real corruption.
            Err(_) if i + 1 == lines.len() && !terminated => break,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(records)
}

/// Makes an unterminated final line safe to append after.
///
/// Without this the next record is written onto the end of the torn line and both are lost:
/// one line holding half a record followed by a whole one parses as neither.
///
/// Two cases, and they are not the same. If the trailing bytes parse as a record then the
/// record itself was written and only the newline was not, so the newline is added and the
/// record is kept. If they do not parse, the write was interrupted part way through and there
/// is nothing there to keep, so it is truncated away.
fn repair_torn_tail(path: &Path) -> Result<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if text.is_empty() || text.ends_with('\n') {
        return Ok(());
    }

    let cut = text.rfind('\n').map_or(0, |i| i + 1);
    let tail = &text[cut..];

    if serde_json::from_str::<Record>(tail.trim()).is_ok() {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
        file.write_all(b"\n")?;
        file.flush()?;
        return Ok(());
    }

    let file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.set_len(cut as u64)?;
    Ok(())
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

    /// Chops `n` bytes off the end, which is what a power loss part way through a write
    /// leaves behind.
    fn chop(path: &std::path::Path, n: u64) {
        let len = std::fs::metadata(path).unwrap().len();
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_len(len - n).unwrap();
    }

    /// The bug. `read` collected every line into one `Result`, so one half written last line
    /// made the whole log unreadable, and `Ledger::open` inherits that, so the daemon refused
    /// to boot at all. The agents from the previous run were then never adopted, never stopped
    /// and never recorded as lost, on that boot or any later one, without a human editing the
    /// log by hand.
    #[test]
    fn a_torn_final_line_is_dropped_rather_than_making_the_log_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let mut ledger = Ledger::open(&path).unwrap();
        ledger.append(1, id("a"), started(10)).unwrap();
        ledger.append(2, id("b"), started(11)).unwrap();
        chop(&path, 20);

        let back = read(&path).unwrap();
        assert_eq!(back.len(), 1, "the whole first record must survive");
        assert_eq!(back[0].seq, 1);
    }

    /// And the daemon side of the same thing: opening has to work, and the sequence has to
    /// carry on from the record that survived rather than from the one that did not.
    #[test]
    fn a_torn_log_can_still_be_opened_and_appended_to() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let mut ledger = Ledger::open(&path).unwrap();
        ledger.append(1, id("a"), started(10)).unwrap();
        ledger.append(2, id("b"), started(11)).unwrap();
        chop(&path, 20);

        let mut reopened = Ledger::open(&path).unwrap();
        assert_eq!(reopened.append(3, id("c"), started(12)).unwrap().seq, 2);

        // Two whole records and nothing glued together. Without the repair the new record
        // would have been written onto the end of the torn line and both would be lost.
        let back = read(&path).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].seq, 2);
    }

    /// A record whose bytes all landed but whose newline did not is a complete record, so it
    /// is kept and the newline is added. Truncating back to the last newline unconditionally,
    /// which is the obvious repair, would throw it away.
    #[test]
    fn a_final_record_missing_only_its_newline_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let mut ledger = Ledger::open(&path).unwrap();
        ledger.append(1, id("a"), started(10)).unwrap();
        ledger.append(2, id("b"), started(11)).unwrap();
        chop(&path, 1); // just the newline

        let mut reopened = Ledger::open(&path).unwrap();
        assert_eq!(
            reopened.append(3, id("c"), started(12)).unwrap().seq,
            3,
            "the second record was whole, so it must still count"
        );
        assert_eq!(read(&path).unwrap().len(), 3);
    }

    /// Corruption anywhere but the end is still an error. A line that was completed once and
    /// is now unreadable means the log disagrees with what happened, and skipping it quietly
    /// would hide exactly that.
    #[test]
    fn a_corrupt_line_in_the_middle_is_still_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let mut ledger = Ledger::open(&path).unwrap();
        ledger.append(1, id("a"), started(10)).unwrap();
        ledger.append(2, id("b"), started(11)).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<&str> = text.lines().collect();
        lines[0] = "{this was a record once";
        std::fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();

        assert!(read(&path).is_err(), "middle corruption must not be silent");
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
