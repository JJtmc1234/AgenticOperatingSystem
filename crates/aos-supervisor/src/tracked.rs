//! The two kinds of agent a supervisor can be holding.
//!
//! An agent we spawned is our child. We can wait on it and read its exit code.
//!
//! An agent that survived a restart is not. When the old supervisor died the kernel
//! reparented it to init, so init reaps it and its exit code goes there. We can watch it and
//! we can stop it, but we can never learn how it ended. Pretending otherwise would mean
//! reporting an exit code we invented.

use std::process::Child;
use std::time::Duration;

use aos_core::{AgentState, ProcessHandle, Result};

use crate::pidfd::PidFd;
use crate::signal;

pub enum Tracked {
    /// We started it and hold its `Child`.
    Spawned { child: Child, handle: ProcessHandle },
    /// It outlived a previous supervisor. Pinned by descriptor so signals cannot go astray.
    Adopted { pinned: PidFd },
}

impl Tracked {
    pub fn handle(&self) -> ProcessHandle {
        match self {
            Tracked::Spawned { handle, .. } => *handle,
            Tracked::Adopted { pinned } => pinned.handle(),
        }
    }

    pub fn is_adopted(&self) -> bool {
        matches!(self, Tracked::Adopted { .. })
    }

    /// Current state, reaping a spawned child that has already exited.
    pub fn state(&mut self) -> Result<AgentState> {
        match self {
            Tracked::Spawned { child, .. } => match child.try_wait()? {
                Some(status) => Ok(AgentState::Stopped {
                    code: status.code(),
                }),
                None => Ok(AgentState::Running { pid: child.id() }),
            },
            Tracked::Adopted { pinned } => {
                if pinned.is_alive() {
                    Ok(AgentState::Running {
                        pid: pinned.handle().pid,
                    })
                } else {
                    // No code, and that is honest. Init reaped it, so the number went there.
                    Ok(AgentState::Stopped { code: None })
                }
            }
        }
    }

    /// Asks it to stop, then insists after `grace`.
    pub fn stop(&mut self, grace: Duration) -> Result<Option<AgentState>> {
        match self {
            Tracked::Spawned { child, .. } => {
                Ok(
                    signal::stop_child(child, grace)?.map(|status| AgentState::Stopped {
                        code: status.code(),
                    }),
                )
            }
            Tracked::Adopted { pinned } => {
                Ok(signal::stop_pinned(pinned, grace)?
                    .then_some(AgentState::Stopped { code: None }))
            }
        }
    }
}
