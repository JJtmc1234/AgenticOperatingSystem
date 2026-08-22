//! The AOS command panel.
//!
//! Two halves, deliberately separate.
//!
//! **Live state comes from the ledger**, tailed read only. `aosd` serves one connection at a
//! time because it owns process lifetimes, so a panel that held a subscription open would lock
//! out every other client including the cli. The ledger is already the source of truth and it
//! is append only, so reading it gives the panel what the daemon knows without asking the
//! daemon for anything, and without being able to disturb it.
//!
//! **Commands go to the daemon**, over short lived connections, one request each. That keeps
//! the plan then commit handshake exactly where it already is rather than reimplementing it.

pub mod feed;

pub use feed::{Feed, Fresh};
