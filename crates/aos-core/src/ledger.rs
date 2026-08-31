//! The append only log.

use std::io::Write;
use std::path::Path;

use crate::{AgentId, Error, Event, Record, Result};

/// Something the log can be written to and forced onto the device.
///
/// `Write::flush` is not enough and on a `File` it is not anything at all, because a `File`
/// holds no userspace buffer to flush. The bytes reach the kernel page cache and stop there,
/// so a power loss or a panic can drop a record for a process that is genuinely running. That
/// is the exact disagreement between the log and the world that appending first exists to
/// prevent, so the sync is part of what a ledger sink is rather than something a caller
/// remembers to do afterwards.
pub trait Durable: std::io::Write {
    /// Returns once the bytes already written are on the device, not merely accepted.
    fn sync(&mut self) -> std::io::Result<()>;
}

impl Durable for std::fs::File {
    fn sync(&mut self) -> std::io::Result<()> {
        // `sync_data` rather than `sync_all`. The contents have to survive. The metadata does
        // not, because a log with the wrong mtime is still a log that can be read.
        self.sync_data()
    }
}

/// One JSON object per line, only ever appended to.
///
/// Text rather than SQLite on purpose. The log has to be readable with `cat` when the thing
/// that writes it is the thing that is broken.
pub struct Ledger {
    sink: Box<dyn Durable>,
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
        //
        // The file goes into the box below, so the lock is held for exactly as long as the
        // ledger is, and released when it is dropped.
        lock_exclusive(&file, path)?;

        Ok(Self {
            sink: Box::new(file),
            next_seq,
        })
    }

    /// A ledger over any durable sink, starting the sequence at `next_seq`.
    ///
    /// This exists so the two things a real file will not do on demand can be reached on
    /// purpose: a write that fails, and a sync that can be counted. Every belief the runtime
    /// holds is a fold over the log, which makes "the log refused a write" the failure worth
    /// testing hardest. A guard nobody can trigger is a guess, so the seam is part of the
    /// product rather than something bolted on beside it.
    ///
    /// No lock is taken here. A sink is not a file, so there is no second writer to keep out,
    /// and the one caller of this is a test that owns everything it touches.
    pub fn to_sink(sink: Box<dyn Durable>, next_seq: u64) -> Self {
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
        // Before `next_seq` moves, so a failed sync leaves the number unused rather than
        // handing it to a record the caller was told nothing about.
        self.sink.sync()?;
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

    /// What the ledger actually did to its sink, in order. 'w' per write and 's' per sync, so
    /// the ordering can be checked and not just the totals.
    type Seen = std::rc::Rc<std::cell::RefCell<String>>;

    /// Records every call rather than keeping the bytes. The ledger owns its sink, so the tape
    /// is shared through an `Rc` and the test keeps its own handle on it.
    struct Counting(Seen);

    impl std::io::Write for Counting {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().push('w');
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Durable for Counting {
        fn sync(&mut self) -> std::io::Result<()> {
            self.0.borrow_mut().push('s');
            Ok(())
        }
    }

    /// A sink whose sync fails, which is a full or dying disk.
    struct SyncRefuses;

    impl std::io::Write for SyncRefuses {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Durable for SyncRefuses {
        fn sync(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("the device is not accepting writes"))
        }
    }

    /// The bug this guards. `append` called `Write::flush`, which on a `File` does nothing at
    /// all, because a `File` holds no userspace buffer. Every record reached the page cache and
    /// stopped there, so a power loss could drop a `Started` record for a process that was
    /// genuinely running, which is the one disagreement the whole append then act rule exists
    /// to prevent.
    #[test]
    fn every_append_is_synced_after_it_is_written() {
        let seen = Seen::default();
        let mut ledger = Ledger::to_sink(Box::new(Counting(seen.clone())), 1);

        ledger.append(1, id("a"), started(10)).unwrap();
        ledger.append(2, id("b"), started(11)).unwrap();

        // Two records, each written and then synced, in that order. Counting alone would pass
        // against a version that synced twice at the end and left the first record exposed in
        // between, so the tape checks the ordering too.
        assert_eq!(
            *seen.borrow(),
            "wsws",
            "every record must be synced immediately after its own write"
        );
    }

    /// A record that could not be forced to the device is not a record. Reporting it as written
    /// would let the caller act on a log entry that a crash can still take away.
    #[test]
    fn a_failed_sync_fails_the_append_and_does_not_burn_the_number() {
        let mut ledger = Ledger::to_sink(Box::new(SyncRefuses), 1);

        assert!(ledger.append(1, id("a"), started(10)).is_err());

        // The sequence must not have moved. If it had, the next record to be written would
        // carry seq 2 with nothing holding seq 1, and the log would have a hole it could not
        // explain.
        let mut ledger = Ledger::to_sink(Box::new(Counting(Seen::default())), ledger.next_seq);
        assert_eq!(ledger.append(2, id("b"), started(11)).unwrap().seq, 1);
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
