//! The append only log.

use std::io::Write;
use std::path::Path;

use crate::{AgentId, Error, Event, Record, Result};

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
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // The maximum, not the last. `last` assumes the file is in order, and the whole
        // reason this bug exists is a file that was not: two writers had already put two
        // records at the same number. Taking the maximum means a reopen can only ever move
        // forwards, so a log that has been damaged stops getting worse. See bug 8.
        let next_seq = read(path)?.iter().map(|r| r.seq).max().map_or(1, |s| s + 1);

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        // One writer at a time, refused rather than waited for.
        //
        // `next_seq` is cached for the life of this handle, which is correct only while this
        // process is the only one appending. A second writer does not corrupt a line, it forks
        // the numbering, and every record either side of the fork looks perfectly well formed.
        // That is the worst shape of corruption available here, because nothing downstream can
        // detect it.
        //
        // Non blocking, so a second `aosd` or an `aos run` against a run directory that already
        // has an owner is told immediately rather than hanging on a lock nobody will release.
        lock_exclusive(&file, path)?;

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

/// Takes an exclusive advisory lock, or says who has it.
///
/// `flock` is per open file description, so the lock lives exactly as long as the `Ledger`
/// holding it, and is released by the kernel if the process dies. There is nothing to clean up
/// after a crash, which is why this rather than a lock file.
fn lock_exclusive(file: &std::fs::File, path: &Path) -> Result<()> {
    use std::os::unix::io::AsRawFd;
    // Sound because `file` owns the descriptor and outlives this call.
    let taken = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if taken == 0 {
        return Ok(());
    }

    Err(Error::Refused(format!(
        "something else is already writing {}. Two writers do not corrupt a line, they fork \
         the sequence numbering, and every record either side of the fork looks well formed, \
         so nothing downstream can tell. Refusing rather than joining in.",
        path.display()
    )))
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

        // Dropped first, because that is what a restart is. The lock lives with the handle,
        // so holding both at once is not a restart, it is two daemons, and that is refused by
        // the test below.
        drop(ledger);

        // A restart must not reset the sequence, or two records share a number and the log
        // stops being an ordering.
        let mut reopened = Ledger::open(&path).unwrap();
        assert_eq!(reopened.append(3, id("c"), started(12)).unwrap().seq, 3);
        assert_eq!(read(&path).unwrap().len(), 3);
    }

    /// The bug. `serve::run` booted the daemon before binding the socket, so a second `aosd`
    /// replayed the log and appended `lost_while_unsupervised` records before discovering a
    /// live daemon and exiting. It had already spent numbers the live daemon believed were
    /// free, and that daemon's cached `next_seq` was then permanently behind, so every record
    /// it wrote from then on collided with one already in the file.
    ///
    /// Binding first closes the ordinary route in. This closes the rest: any second writer,
    /// including `aos run` against a directory a daemon already owns.
    #[test]
    fn a_second_writer_is_refused_rather_than_forking_the_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let _held = Ledger::open(&path).unwrap();

        let e = match Ledger::open(&path) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("a second writer was let in"),
        };
        assert!(e.contains("already writing"), "{e}");
        assert!(e.contains("fork the sequence"), "and says why: {e}");
    }

    /// And the lock goes with the handle, so a daemon that has stopped leaves nothing to clean
    /// up. `flock` is released by the kernel when the descriptor closes, including on a crash,
    /// which is why this is a lock on the file rather than a lock file.
    #[test]
    fn the_lock_is_released_when_the_ledger_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let held = Ledger::open(&path).unwrap();
        drop(held);
        assert!(
            Ledger::open(&path).is_ok(),
            "a stopped daemon must not lock others out"
        );
    }

    /// A log that already holds two records at the same number is damaged, and reopening it
    /// must not make it worse. `last` would hand back a number already used if the damaged
    /// record happened to be last; the maximum can only ever move forwards.
    #[test]
    fn reopening_a_damaged_log_never_goes_backwards() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        {
            let mut ledger = Ledger::open(&path).unwrap();
            ledger.append(1, id("a"), started(10)).unwrap();
            ledger.append(2, id("b"), started(11)).unwrap();
        }

        // A record out of order, which is exactly what the two writer bug left behind.
        let text = std::fs::read_to_string(&path).unwrap();
        let first = text.lines().next().unwrap().to_string();
        std::fs::write(&path, format!("{text}{first}\n")).unwrap();

        let mut reopened = Ledger::open(&path).unwrap();
        assert_eq!(
            reopened.append(3, id("c"), started(12)).unwrap().seq,
            3,
            "the next number has to be past everything already used, not past the last line"
        );
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
