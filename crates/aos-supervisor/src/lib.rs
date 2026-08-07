//! Starts, tracks and stops agents as ordinary Linux processes.
//!
//! Deliberately boring. An agent is a child process, nothing more. Anything clever belongs
//! above this layer, so that the part which can leave stray processes on the machine stays
//! small enough to read in one sitting.

pub mod pidfd;
pub mod proc;
pub mod replay;
mod signal;
mod spawn;
mod tracked;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use aos_core::{AgentId, AgentSpec, AgentState, Error, ProcessHandle, Record, Result};

pub use pidfd::PidFd;
pub use replay::{Recovered, recover};
pub use signal::StopMode;
pub use tracked::Tracked;

/// Owns every agent this supervisor is responsible for. Dropping it does not kill them,
/// because a supervisor that takes its agents down when the caller goes away cannot be a
/// daemon later.
pub struct Supervisor {
    agents: BTreeMap<AgentId, Tracked>,
    allowed: Vec<String>,
    log_dir: PathBuf,
}

impl Supervisor {
    /// `allowed` is the list of programs that may be launched, matched exactly.
    ///
    /// Never put an interpreter on it. `python -c` and `node -e` take code on their own
    /// argument vector, so allowing one grants everything the other gates protect.
    ///
    /// `log_dir` receives one combined output file per agent. It is not optional, because the
    /// alternative is a pipe nobody drains, which loses the output and eventually deadlocks
    /// the agent. See bug 1 in `bug-list.md`.
    pub fn new(allowed: impl IntoIterator<Item = String>, log_dir: impl Into<PathBuf>) -> Self {
        Self {
            agents: BTreeMap::new(),
            allowed: allowed.into_iter().collect(),
            log_dir: log_dir.into(),
        }
    }

    /// Path this agent's combined stdout and stderr are appended to.
    pub fn log_path(&self, id: &AgentId) -> PathBuf {
        // Safe to join directly. AgentId refuses separators and dot segments at construction.
        self.log_dir.join(format!("{id}.log"))
    }

    /// Launches an agent and returns the handle that identifies its process.
    pub fn start(&mut self, spec: &AgentSpec) -> Result<ProcessHandle> {
        if self.agents.contains_key(&spec.id) {
            return Err(Error::Refused(format!("{} is already running", spec.id)));
        }

        let log_file = self.log_path(&spec.id);
        let (child, handle) = spawn::launch(spec, &self.allowed, &self.log_dir, &log_file)?;

        self.agents
            .insert(spec.id.clone(), Tracked::Spawned { child, handle });
        Ok(handle)
    }

    /// Takes responsibility for an agent that outlived a previous supervisor.
    ///
    /// Refuses unless the process is genuinely still the one recorded. That check and the
    /// pinning happen together in `PidFd::open`, so nothing can slip in between them.
    pub fn adopt(&mut self, id: AgentId, handle: ProcessHandle) -> Result<()> {
        if self.agents.contains_key(&id) {
            return Err(Error::Refused(format!("{id} is already tracked")));
        }
        let pinned = PidFd::open(handle).ok_or_else(|| {
            Error::Refused(format!(
                "{id} is not running as pid {}, so it will not be adopted",
                handle.pid
            ))
        })?;

        self.agents.insert(id, Tracked::Adopted { pinned });
        Ok(())
    }

    /// Replays a log and adopts everything it says is running that genuinely still is.
    ///
    /// Returns what recovery found, so the caller can write a record for each agent that was
    /// lost while nobody was supervising. Closing that gap in the log is the caller's job,
    /// because only the caller holds the ledger.
    pub fn adopt_from(&mut self, records: &[Record]) -> Recovered {
        let recovered = recover(records);
        for (id, handle) in &recovered.alive {
            // A failure here means it died between recovery and adoption. Rare, and the agent
            // simply stays untracked, which is the safe outcome.
            let _ = self.adopt(id.clone(), *handle);
        }
        recovered
    }

    /// Current state of one agent, reaping it if it has already exited.
    pub fn state(&mut self, id: &AgentId) -> Result<AgentState> {
        let tracked = self
            .agents
            .get_mut(id)
            .ok_or_else(|| Error::UnknownAgent(id.clone()))?;

        let state = tracked.state()?;
        if matches!(state, AgentState::Stopped { .. }) {
            self.agents.remove(id);
        }
        Ok(state)
    }

    pub fn list(&mut self) -> Vec<(AgentId, AgentState)> {
        let ids: Vec<_> = self.agents.keys().cloned().collect();
        ids.into_iter()
            .filter_map(|id| self.state(&id).ok().map(|s| (id, s)))
            .collect()
    }

    /// Whether this agent was inherited rather than started by us.
    pub fn is_adopted(&self, id: &AgentId) -> bool {
        self.agents.get(id).is_some_and(Tracked::is_adopted)
    }

    /// Asks the agent to stop, then insists after `grace`.
    ///
    /// The wait is bounded. An unbounded wait on a child that ignores the signal hangs the
    /// supervisor, which is the failure the Windows AOS hit and guarded against.
    pub fn stop(&mut self, id: &AgentId, grace: Duration) -> Result<AgentState> {
        let tracked = self
            .agents
            .get_mut(id)
            .ok_or_else(|| Error::UnknownAgent(id.clone()))?;

        let Some(state) = tracked.stop(grace)? else {
            return Err(Error::Refused(format!("{id} did not stop")));
        };

        self.agents.remove(id);
        Ok(state)
    }

    /// Kill switch. Stops every agent regardless of tier and reports what it stopped.
    #[allow(clippy::type_complexity)]
    pub fn stop_all(&mut self, grace: Duration) -> Vec<(AgentId, Result<AgentState>)> {
        let ids: Vec<_> = self.agents.keys().cloned().collect();
        ids.into_iter()
            .map(|id| {
                let outcome = self.stop(&id, grace);
                (id, outcome)
            })
            .collect()
    }
}
