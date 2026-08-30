//! The one thread that is allowed to block.
//!
//! `aosd` serves one connection at a time, so a request can wait on whatever the daemon is
//! already doing. A UI that made that call inline would freeze for exactly as long, which is
//! how a panel ends up looking crashed while the system underneath it is fine. So orders go
//! down a channel, one thread runs them, and the answers come back up another channel and are
//! applied on the next frame.
//!
//! The thread also pings on its own while it has nothing else to do, which is what makes the
//! connection state on screen a measurement rather than a guess.
//!
//! Nothing here writes to the ledger. Every one of these is a request to the daemon or a read
//! of a file the daemon owns.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;

use aos_core::{AgentId, AgentSpec, PlanId, RiskTier, Verdict};

use crate::link::Link;

mod calls;
#[cfg(test)]
mod tests;

/// How often an idle worker asks whether the daemon is still there.
const HEARTBEAT: Duration = Duration::from_secs(2);

/// Something a person asked for. One order in, one batch of outcomes out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    Ping,
    Stop {
        agent: AgentId,
        grace_secs: u64,
    },
    StopAll {
        grace_secs: u64,
    },
    /// Read a spec off disk and work out what starting it would take. Touches no daemon.
    LoadSpec(PathBuf),
    /// The planning call. A start with no commit quoted, so above tier read nothing runs.
    Plan(Box<AgentSpec>),
    /// The committing call, quoting a plan the daemon offered for this exact spec.
    Commit {
        spec: Box<AgentSpec>,
        plan: PlanId,
    },
}

/// What came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The worker asked on its own. Does not settle anything a person is waiting on.
    Heartbeat(Link),
    SpecLoaded {
        spec: Box<AgentSpec>,
        /// What `run_dir/policy.toml` says about this agent at this tier. The daemon has the
        /// final say and may hold an older policy, so this is what the button is labelled
        /// with and never what it relies on.
        verdict: Option<Verdict>,
    },
    /// The daemon offered a plan and did nothing. This is the half that needs a second click.
    PlanOffered {
        plan: PlanId,
        agent: AgentId,
        tier: RiskTier,
        summary: String,
    },
    Note {
        text: String,
        tone: Tone,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Good,
    Bad,
}

impl Outcome {
    /// Whether this answers something a person asked for, as opposed to the worker's own ping.
    pub const fn settles_an_order(&self) -> bool {
        !matches!(self, Outcome::Heartbeat(_))
    }

    pub(super) fn bad(text: impl std::fmt::Display) -> Self {
        Outcome::Note {
            text: text.to_string(),
            tone: Tone::Bad,
        }
    }

    pub(super) fn good(text: impl std::fmt::Display) -> Self {
        Outcome::Note {
            text: text.to_string(),
            tone: Tone::Good,
        }
    }
}

/// The UI side of the worker.
pub struct Worker {
    orders: Sender<Order>,
    incoming: Receiver<Outcome>,
}

impl Worker {
    pub fn spawn(run_dir: PathBuf) -> Self {
        let (orders, take_orders) = channel::<Order>();
        let (replies, incoming) = channel::<Outcome>();

        std::thread::Builder::new()
            .name("aos-panel-worker".into())
            .spawn(move || calls::serve(&run_dir, &take_orders, &replies))
            .expect("the worker thread could not be started");

        Self { orders, incoming }
    }

    /// Queues an order. Returns false only if the worker thread has gone, which the caller
    /// shows rather than swallows.
    pub fn send(&self, order: Order) -> bool {
        self.orders.send(order).is_ok()
    }

    /// Everything that has come back since the last call. Never blocks, so it is safe in a
    /// draw loop.
    pub fn drain(&self) -> Vec<Outcome> {
        let mut out = Vec::new();
        loop {
            match self.incoming.try_recv() {
                Ok(outcome) => out.push(outcome),
                Err(TryRecvError::Empty) => return out,
                Err(TryRecvError::Disconnected) => {
                    out.push(Outcome::bad(
                        "the worker thread has gone, so nothing can be sent",
                    ));
                    return out;
                }
            }
        }
    }
}
