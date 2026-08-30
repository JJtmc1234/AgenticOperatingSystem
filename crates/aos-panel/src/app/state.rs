//! The values the panel holds, with no behaviour attached.
//!
//! Split out so the shapes can be read in one screen. Every one of them has a case for not
//! knowing, because the panel is often looking at a system that has not told it anything yet.

use std::time::{SystemTime, UNIX_EPOCH};

use aos_core::{AgentId, AgentSpec, PlanId, RiskTier, Verdict};

use crate::worker::Tone;

/// How many command results are kept. Enough to see what just happened, few enough that the
/// screen does not become a second log competing with the real one.
pub const NOTES_KEPT: usize = 8;

/// One thing the panel did or was told, with when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub at: u64,
    pub text: String,
    pub tone: Tone,
}

/// What the panel last managed to read out of the ledger.
#[derive(Debug, Clone, Default)]
pub struct LedgerStatus {
    /// When the last read succeeded. `None` means not once yet, which is not the same as
    /// "read and found nothing".
    pub read_at: Option<u64>,
    pub reads: u64,
    /// The file was replaced under us this many times. Visible because it means the history
    /// on screen is not the history that was there before.
    pub restarts: u64,
    /// Why the last read failed, when it did. Cleared by the next read that works.
    pub error: Option<String>,
    /// Whether the file is there at all. A run directory with no ledger yet is a normal state
    /// and reads differently from an empty one.
    pub exists: bool,
}

/// Where a start has got to. The whole point of the type is that the committing step cannot
/// be reached except through the planning step.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StartFlow {
    #[default]
    Idle,
    /// A spec was read off disk. Nothing has been sent anywhere.
    Loaded {
        spec: Box<AgentSpec>,
        verdict: Option<Verdict>,
    },
    /// The planning call is out.
    Planning { spec: Box<AgentSpec> },
    /// The daemon offered a plan and did nothing. This is the state that has a commit button,
    /// and it is the only one.
    Offered {
        /// The exact spec that was planned. The commit quotes this and not whatever is in the
        /// text box now, because the daemon compares the whole spec and a plan for one thing
        /// must not commit another.
        spec: Box<AgentSpec>,
        plan: PlanId,
        agent: AgentId,
        tier: RiskTier,
        summary: String,
    },
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}
