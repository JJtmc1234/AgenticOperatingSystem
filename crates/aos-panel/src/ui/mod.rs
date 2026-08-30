//! Drawing, and nothing else.
//!
//! Every function here reads the `App` and puts input back through its methods. None of them
//! hold state, decide anything or touch a socket, which is what keeps the rules in one place
//! where they can be tested without a window.

pub mod agents;
pub mod commands;
pub mod events;
pub mod shell;
pub mod start;
pub mod widgets;

#[cfg(test)]
mod tests;

pub use shell::draw;
