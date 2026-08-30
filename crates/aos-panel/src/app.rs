//! Everything the panel knows, and nothing that draws.
//!
//! Kept away from egui on purpose, so the rules can be tested without a window. A panel breaks
//! in two different places: in the drawing, where a row will not fit, and in the state, where
//! it says something untrue. Only the second kind is worth a unit test, and it is only
//! testable if no widget is involved.
//!
//! Two sources feed this, and they fail apart from each other. The **ledger** is a file, read
//! only, and it keeps working with nothing running. The **daemon** is a socket, reached only
//! through the worker thread, and it is the only thing that can act.

use std::path::{Path, PathBuf};

use aos_core::{AgentId, Record};

use crate::feed::Feed;
use crate::link::Link;
use crate::rows::{self, AgentRow};
use crate::worker::{Order, Tone, Worker};

mod apply;
mod commands;
mod state;
#[cfg(test)]
mod tests;

pub use state::{LedgerStatus, NOTES_KEPT, Note, StartFlow, now};

pub struct App {
    run_dir: PathBuf,
    feed: Feed,
    /// The whole ledger, in order. Kept whole because the row fold is a fold over all of it,
    /// and a fold over the tail would report a restarted agent as never started.
    records: Vec<Record>,
    rows: Vec<AgentRow>,
    worker: Worker,

    pub link: Link,
    pub ledger: LedgerStatus,
    pub notes: Vec<Note>,
    pub start: StartFlow,
    /// The order a person is waiting on. One at a time, which is also what stops a second
    /// click sending a second stop.
    pub in_flight: Option<Order>,
    pub spec_path: String,
    pub grace_secs: u64,
    pub newest_first: bool,
    pub refusals_only: bool,
    pub selected: Option<AgentId>,
    /// Unix seconds, taken once per tick so everything in one frame agrees about the time.
    pub now: u64,
}

impl App {
    pub fn new(run_dir: PathBuf) -> Self {
        Self {
            feed: Feed::new(&run_dir),
            worker: Worker::spawn(run_dir.clone()),
            run_dir,
            records: Vec::new(),
            rows: Vec::new(),
            link: Link::Unknown,
            ledger: LedgerStatus::default(),
            notes: Vec::new(),
            start: StartFlow::Idle,
            in_flight: None,
            spec_path: String::new(),
            grace_secs: 5,
            newest_first: true,
            refusals_only: false,
            selected: None,
            now: now(),
        }
    }

    /// One pass: reread the ledger, then apply whatever the worker sent back.
    ///
    /// The ledger read is a `stat` and a read of the bytes appended since last time, which is
    /// cheap enough to do every frame. Anything that could block is on the worker thread.
    pub fn tick(&mut self) {
        self.now = now();
        self.read_ledger();
        for outcome in self.worker.drain() {
            self.apply(outcome);
        }
    }

    fn read_ledger(&mut self) {
        self.ledger.exists = self.feed.path().exists();
        match self.feed.read() {
            Ok(fresh) => {
                let restarted = fresh.restarted;
                let arrived = !fresh.records.is_empty();
                if restarted {
                    self.records.clear();
                    self.note(
                        "the ledger was replaced, so everything before this is gone",
                        Tone::Bad,
                    );
                    self.ledger.restarts += 1;
                }
                self.records.extend(fresh.records);
                if restarted || arrived {
                    self.rows = rows::fold(&self.records);
                }
                self.ledger.read_at = Some(self.now);
                self.ledger.reads += 1;
                self.ledger.error = None;
            }
            // Kept as an error rather than as an empty ledger. A file that cannot be read is
            // not a file with nothing in it.
            Err(e) => self.ledger.error = Some(e.to_string()),
        }
    }

    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    pub fn ledger_path(&self) -> &Path {
        self.feed.path()
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }

    pub fn rows(&self) -> &[AgentRow] {
        &self.rows
    }

    pub fn last_seq(&self) -> u64 {
        self.feed.last_seq()
    }

    pub fn running(&self) -> usize {
        self.rows.iter().filter(|r| r.life.is_running()).count()
    }

    pub fn refusals(&self) -> usize {
        self.records
            .iter()
            .filter(|r| crate::format::is_refusal(r))
            .count()
    }
}
