//! Starts, tracks and stops agents as ordinary Linux processes.
//!
//! Deliberately boring. An agent is a child process, nothing more. Anything clever belongs
//! above this layer, so that the part which can leave stray processes on the machine stays
//! small enough to read in one sitting.

mod signal;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use aos_core::{AgentId, AgentSpec, AgentState, Error, Result};

pub use signal::StopMode;

/// Owns every running child. Dropping it does not kill them, because a supervisor that
/// takes its agents down when the caller goes away cannot be a daemon later.
pub struct Supervisor {
    children: BTreeMap<AgentId, Child>,
    allowed: Vec<String>,
    log_dir: PathBuf,
}

impl Supervisor {
    /// `allowed` is the list of programs that may be launched, matched exactly.
    ///
    /// Never put an interpreter on it. `python -c` and `node -e` take code on their own
    /// argument vector, so allowing one grants everything the other gates protect.
    ///
    /// `log_dir` receives one combined output file per agent. It is not optional, because
    /// the alternative is a pipe nobody drains, which loses the output and eventually
    /// deadlocks the agent. See bug 1 in `bug-list.md`.
    pub fn new(allowed: impl IntoIterator<Item = String>, log_dir: impl Into<PathBuf>) -> Self {
        Self {
            children: BTreeMap::new(),
            allowed: allowed.into_iter().collect(),
            log_dir: log_dir.into(),
        }
    }

    /// Path this agent's combined stdout and stderr are appended to.
    pub fn log_path(&self, id: &AgentId) -> PathBuf {
        // Safe to join directly. AgentId refuses separators and dot segments at construction.
        self.log_dir.join(format!("{id}.log"))
    }

    pub fn start(&mut self, spec: &AgentSpec) -> Result<AgentState> {
        if self.children.contains_key(&spec.id) {
            return Err(Error::Refused(format!("{} is already running", spec.id)));
        }
        if !self.allowed.iter().any(|p| p == &spec.program) {
            return Err(Error::Refused(format!(
                "{:?} is not an allowed program, allowed are {:?}",
                spec.program, self.allowed
            )));
        }

        let log = open_log(&self.log_dir, &self.log_path(&spec.id))?;

        // Arguments go straight to execve as a list. No shell, so a semicolon or a pipe
        // inside an argument stays literal text.
        let child = Command::new(&spec.program)
            .args(&spec.args)
            // The child must not inherit our stdin. On the Windows AOS a child inherited the
            // protocol pipe and ate the parent's messages.
            .stdin(Stdio::null())
            // A file, never a pipe. Nothing here drains a pipe, and a full pipe buffer stops
            // the agent dead at roughly 64 KB of output.
            .stderr(Stdio::from(log.try_clone()?))
            .stdout(Stdio::from(log))
            .spawn()?;

        let pid = child.id();
        self.children.insert(spec.id.clone(), child);
        Ok(AgentState::Running { pid })
    }

    /// Current state of one agent, reaping it if it has already exited.
    pub fn state(&mut self, id: &AgentId) -> Result<AgentState> {
        let child = self
            .children
            .get_mut(id)
            .ok_or_else(|| Error::UnknownAgent(id.clone()))?;

        match child.try_wait()? {
            Some(status) => {
                self.children.remove(id);
                Ok(AgentState::Stopped {
                    code: status.code(),
                })
            }
            None => Ok(AgentState::Running { pid: child.id() }),
        }
    }

    pub fn list(&mut self) -> Vec<(AgentId, AgentState)> {
        let ids: Vec<_> = self.children.keys().cloned().collect();
        ids.into_iter()
            .filter_map(|id| self.state(&id).ok().map(|s| (id, s)))
            .collect()
    }

    /// Asks the agent to stop, then insists after `grace`.
    ///
    /// The wait is bounded. An unbounded wait on a child that ignores the signal hangs the
    /// supervisor, which is the failure the Windows AOS hit and guarded against.
    pub fn stop(&mut self, id: &AgentId, grace: Duration) -> Result<AgentState> {
        let child = self
            .children
            .get_mut(id)
            .ok_or_else(|| Error::UnknownAgent(id.clone()))?;

        let status = match signal::stop_child(child, grace)? {
            Some(status) => status,
            None => return Err(Error::Refused(format!("{id} did not stop"))),
        };

        self.children.remove(id);
        Ok(AgentState::Stopped {
            code: status.code(),
        })
    }

    /// Kill switch. Stops every agent regardless of tier and reports what it stopped.
    #[allow(clippy::type_complexity)]
    pub fn stop_all(&mut self, grace: Duration) -> Vec<(AgentId, Result<AgentState>)> {
        let ids: Vec<_> = self.children.keys().cloned().collect();
        ids.into_iter()
            .map(|id| {
                let outcome = self.stop(&id, grace);
                (id, outcome)
            })
            .collect()
    }
}

/// Opens an agent's log for appending, creating the directory on first use.
///
/// Append rather than truncate, so restarting an agent keeps the history that explains why
/// it needed restarting.
fn open_log(dir: &Path, path: &Path) -> Result<std::fs::File> {
    std::fs::create_dir_all(dir)?;
    Ok(std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?)
}
