//! Capability servers, spoken over MCP.
//!
//! Phase 3 of AOS. Claude Code already speaks MCP, so a capability is exposed as an MCP tool
//! rather than as anything new, and the gate is the same tier, policy and plan machinery the
//! supervisor already uses.
//!
//! Four things have to be true of every call, and they are separate on purpose because they
//! fail for different reasons.
//!
//! | | |
//! |---|---|
//! | `root` | the path is inside the directory this server is allowed to touch |
//! | `tools` | the capability exists at all, and declares how much damage it could do |
//! | `policy` | that tier is allowed, for this agent |
//! | `plan` | anything above Read was agreed to before it happened |
//!
//! Never a raw shell. A shell is every tier at once, so it cannot be described to a policy,
//! and there is no entry for one.

pub mod files;
pub mod root;
pub mod rpc;
pub mod server;
pub mod tools;

pub use root::Root;
pub use server::Server;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("refused: {0}")]
    Refused(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Core(#[from] aos_core::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
