//! Safety contracts shared by every AOS component.
//!
//! Nothing here touches the machine. These are the types that describe what an agent is
//! allowed to ask for and what happened when it asked. Keeping them in one crate with no
//! side effects means the rules can be tested without spawning anything.

pub mod agent;
pub mod event;
pub mod fold;
pub mod ledger;
pub mod plan;
pub mod policy;
pub mod protocol;
pub mod tier;

pub use agent::{AgentId, AgentSpec, AgentState};
pub use event::{Event, ProcessHandle, Record};
pub use fold::believed_running;
pub use ledger::Ledger;
pub use plan::{Plan, PlanId, PlanLedger};
pub use policy::{Policy, Verdict};
pub use protocol::{AgentReport, Request, Response};
pub use tier::RiskTier;

/// Errors any component may hand back.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("agent id must be lowercase letters, digits or dashes, got {0:?}")]
    BadAgentId(String),

    #[error("refused by policy: {0}")]
    Refused(String),

    #[error("no agent named {0}")]
    UnknownAgent(AgentId),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
