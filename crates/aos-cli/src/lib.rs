//! The parts of the cli that other programs need.
//!
//! `client` is here rather than private to the binary because the panel talks to the same
//! daemon over the same protocol, and a second copy of a protocol client is a second thing
//! that can drift from the daemon it is talking to. One definition, two callers.
//!
//! Everything else stays private to the binary, because nothing else needs it.

pub mod client;
