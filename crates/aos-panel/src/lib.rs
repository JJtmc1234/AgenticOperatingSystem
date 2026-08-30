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
//! **Commands go to the daemon**, over short lived connections, one request each, on a thread
//! of their own. That keeps the plan then commit handshake exactly where it already is rather
//! than reimplementing it, and keeps a blocking socket call away from the draw loop.
//!
//! The panel is read mostly. It opens the ledger read only and writes nothing to it, ever.

pub mod app;
pub mod feed;
pub mod format;
pub mod link;
pub mod report;
pub mod rows;
pub mod theme;
pub mod ui;
pub mod worker;

pub use app::App;
pub use feed::{Feed, Fresh};
pub use link::Link;
pub use rows::AgentRow;
pub use worker::Worker;
