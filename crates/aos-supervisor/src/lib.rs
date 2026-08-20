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
/// What each agent was last recorded as being started with.
fn started_programs(records: &[Record]) -> BTreeMap<AgentId, String> {
    let mut out = BTreeMap::new();
    for record in records {
        if let aos_core::Event::Started { program, .. } = &record.event {
            out.insert(record.agent.clone(), program.clone());
        }
    }
    out
}

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

        self.agents.insert(
            spec.id.clone(),
            Tracked::Spawned {
                child,
                handle: handle.clone(),
            },
        );
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
        let pid = handle.pid;
        let pinned = PidFd::open(handle).ok_or_else(|| {
            Error::Refused(format!(
                "{id} is not running as pid {pid}, so it will not be adopted"
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
        let mut recovered = recover(records);

        // What each agent was recorded as running, so adoption can check the pid is still that
        // program. The handle alone says which process; it says nothing about what it is.
        let programs = started_programs(records);

        let mut refused = Vec::new();
        for (id, handle) in &recovered.alive {
            if let Some(why) = self.wrong_program(id, handle, &programs) {
                eprintln!("WARNING: not adopting {id}: {why}");
                refused.push(id.clone());
                continue;
            }
            // Kept, not dropped. This used to be `let _ =`, and the comment above it said a
            // failure means the agent died between recovery and adoption, so staying untracked
            // was safe. That is one cause among several and the only harmless one.
            //
            // Any other cause leaves a live agent that the supervisor is not tracking while
            // boot reports it adopted and writes no record at all. `aos list` cannot see it and
            // `stop-all` cannot reach it. Worse, its id is free, so a later `aos start` under
            // the same id appends a second `Started` that supersedes the first in
            // `believed_running`, and no later boot ever looks at the original process again.
            //
            // A kernel or seccomp profile without `pidfd_open` fails every adoption on every
            // boot. Descriptor pressure does it too, because `PidFd::open` holds a descriptor
            // across its second check, so one free descriptor is enough to open and not enough
            // to recheck. See bug 9.
            if let Err(e) = self.adopt(id.clone(), handle.clone()) {
                eprintln!("WARNING: not adopting {id}: {e}");
                refused.push(id.clone());
            }
        }

        // Moved rather than dropped, and into `unknown` rather than `lost`, whichever check
        // refused it. A live agent must not be recorded as dead, and adoption failing does not
        // say the agent is gone: it says this supervisor could not take it back.
        for id in refused {
            if let Some(handle) = recovered.alive.remove(&id) {
                recovered.unknown.push((id, handle));
            }
        }
        recovered
    }

    /// Why this pid should not be adopted under this agent id, if there is a reason.
    ///
    /// Two checks the old code did neither of. The handle identifies a process and says nothing
    /// about what that process is, so adoption used to re-admit anything that happened to be on
    /// the pid, including a program the allowlist no longer permits. That needs no pid
    /// collision at all: take a program off the allowlist, restart the daemon, and the running
    /// agent is adopted straight back. See bug 8.
    fn wrong_program(
        &self,
        id: &AgentId,
        handle: &ProcessHandle,
        programs: &BTreeMap<AgentId, String>,
    ) -> Option<String> {
        let recorded = programs.get(id)?;

        // The allowlist is the current rule, not the one that applied when this started.
        if !self.allowed.iter().any(|p| p == recorded) {
            return Some(format!(
                "it was started as {recorded}, which is no longer an allowed program"
            ));
        }

        match proc::program_of(handle.pid) {
            // Compared after resolving both, so a symlinked path does not read as a mismatch.
            Some(running) => {
                let recorded_real = std::fs::canonicalize(recorded).ok();
                let running_real = std::fs::canonicalize(&running).ok();
                if recorded_real.is_some() && recorded_real == running_real {
                    None
                } else {
                    Some(format!(
                        "pid {} is running {}, not the {recorded} it was recorded as",
                        handle.pid,
                        running.display()
                    ))
                }
            }
            // Cannot see what it is running. Not adopting is the safe answer, for the same
            // reason an unreadable `/proc` is not treated as a dead agent.
            None => Some(format!(
                "pid {} cannot be read, so what it is running is unknown",
                handle.pid
            )),
        }
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
