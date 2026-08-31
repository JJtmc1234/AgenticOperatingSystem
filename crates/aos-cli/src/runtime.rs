//! Loading specs and running one in the foreground.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use aos_core::{AgentSpec, AgentState, Event, Ledger};
use aos_supervisor::Supervisor;

pub fn load_spec(path: &Path) -> Result<AgentSpec> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("{} is not a valid agent spec", path.display()))
}

pub fn log_path(run_dir: &Path) -> PathBuf {
    run_dir.join("events.jsonl")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

pub fn run(run_dir: &Path, spec_path: &Path) -> Result<()> {
    let spec = load_spec(spec_path)?;
    let allowed = crate::allowlist(run_dir)?;
    let ledger = Ledger::open(log_path(run_dir))?;
    let sup = Supervisor::new(allowed, run_dir.join("logs"));

    supervise(ledger, sup, spec)
}

/// Runs one agent to completion against a ledger and supervisor already built.
///
/// Split out from `run` so a test can hand it a log that refuses writes, which is the failure
/// the recovery below exists for and the one a real file will not produce on demand.
fn supervise(mut ledger: Ledger, mut sup: Supervisor, spec: AgentSpec) -> Result<()> {
    // A refusal is written too, because a log that only records what worked hides exactly the
    // calls worth reviewing.
    //
    // A start cannot be written before it happens. The pid and its start token do not exist
    // until the child does, so there is nothing truthful to append beforehand. The rule
    // instead is that a start which could not be recorded is undone, because a running agent
    // nobody wrote down is one nothing on this machine can find, stop or account for. `aosd`
    // does the same thing in `Daemon::launch`.
    let handle = match sup.start(&spec) {
        Ok(handle) => {
            let recorded = ledger.append(
                now(),
                spec.id.clone(),
                Event::Started {
                    handle,
                    program: spec.program.clone(),
                },
            );
            if let Err(e) = recorded {
                let stopped = sup.stop(&spec.id, Duration::from_secs(5));
                anyhow::bail!(
                    "started {} but could not record it, so it was stopped again: {e}{}",
                    spec.id,
                    match stopped {
                        Ok(_) => String::new(),
                        // Worth saying loudly. Unrecorded and still running is the state this
                        // whole path exists to prevent, and now only a person can close it.
                        Err(e) => format!(
                            ". Stopping it failed too, so pid {} may still be running and \
                             nothing has recorded it: {e}",
                            handle.pid
                        ),
                    }
                );
            }
            handle
        }
        Err(err) => {
            ledger.append(
                now(),
                spec.id.clone(),
                Event::Refused {
                    reason: err.to_string(),
                },
            )?;
            return Err(err.into());
        }
    };

    println!(
        "{} running as pid {}, output at {}",
        spec.id,
        handle.pid,
        sup.log_path(&spec.id).display()
    );

    loop {
        match sup.state(&spec.id)? {
            AgentState::Stopped { code } => {
                ledger.append(now(), spec.id.clone(), Event::Exited { code })?;
                println!("{} stopped, exit code {code:?}", spec.id);
                return Ok(());
            }
            AgentState::Running { .. } => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// Reconciles the log against `/proc` and reports what is genuinely still running.
///
/// This is what a daemon will do on boot. Exposing it as a command first means the recovery
/// logic is exercised by hand before anything depends on it.
pub fn status(run_dir: &Path) -> Result<()> {
    let records = aos_core::ledger::read(log_path(run_dir))?;
    let recovered = aos_supervisor::recover(&records);

    println!("{} records in the log", records.len());

    if recovered.alive.is_empty() && recovered.lost.is_empty() {
        println!("nothing was left running");
        return Ok(());
    }

    for (agent, handle) in &recovered.alive {
        println!("alive  {agent}  pid {}", handle.pid);
    }
    for (agent, handle) in &recovered.lost {
        println!(
            "lost   {agent}  pid {} is gone or was recycled, so it will not be touched",
            handle.pid
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_core::{AgentId, RiskTier};

    /// A sink that refuses every write, which is what a full disk looks like from here.
    struct Refusing;

    impl std::io::Write for Refusing {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("no space left on device"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Every live process whose command line mentions `marker`.
    ///
    /// Read from `/proc` rather than shelled out to `pgrep`, so the test depends on nothing
    /// that might not be installed. A reaped or zombie process has an empty command line, so
    /// only something genuinely still there can match.
    fn survivors(marker: &str) -> Vec<u32> {
        let mut found = Vec::new();
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return found;
        };
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            if let Ok(cmdline) = std::fs::read(entry.path().join("cmdline"))
                && String::from_utf8_lossy(&cmdline).contains(marker)
            {
                found.push(pid);
            }
        }
        found
    }

    /// The bug this guards. `run` started the child, then appended with a bare `?`, so a log
    /// that would not take the write left a process running that nothing had recorded. No pid
    /// anywhere, so no later boot, `status` or kill switch could ever find it.
    ///
    /// The marker is a long sleep with this process id after the decimal point. It has to be
    /// unique on the whole machine, because `survivors` matches a substring of every command
    /// line, and the first version used a bare `4919` which matched an unrelated shell command
    /// that happened to mention it. A guard that fails for reasons other than the bug is worse
    /// than no guard.
    #[test]
    fn a_start_that_cannot_be_recorded_leaves_no_surviving_child() {
        let marker = format!("4919.{}", std::process::id());
        let marker = marker.as_str();
        let dir = tempfile::tempdir().unwrap();

        assert!(
            survivors(marker).is_empty(),
            "something already matches {marker}, so this test cannot prove anything"
        );

        let spec = AgentSpec {
            id: AgentId::new("sleeper").unwrap(),
            program: "/usr/bin/sleep".into(),
            args: vec![marker.into()],
            ceiling: RiskTier::Read,
        };
        let sup = Supervisor::new(
            ["/usr/bin/sleep".to_string()],
            dir.path().join("logs").to_path_buf(),
        );

        let outcome = supervise(Ledger::to_sink(Box::new(Refusing), 1), sup, spec);

        // Sampled and cleaned up before anything is asserted, for two reasons. An assertion
        // about the message must not be able to fail first and hide the leak. And against the
        // broken version this test is the thing that leaked the process, so it is the thing
        // that has to clean it up. Bug 3 in this list was exactly that mistake.
        let left_running = survivors(marker);
        for pid in &left_running {
            // Sound because these pids came from `/proc` moments ago and carry a command line
            // this process invented, so the target can only be the child started above. A
            // recycled pid cannot match a marker containing our own pid and a 4919 second
            // sleep. SIGKILL rather than SIGTERM, because this is debris, not a shutdown.
            unsafe { libc::kill(*pid as libc::pid_t, libc::SIGKILL) };
        }

        // The claim that matters, checked first. The message is only worth anything if this
        // holds.
        assert_eq!(
            left_running,
            Vec::<u32>::new(),
            "the child outlived the failed append, which is the whole bug"
        );

        let error = outcome.expect_err("a log that refuses every write must not look like success");
        let error = error.to_string();
        assert!(error.contains("could not record it"), "{error}");
        assert!(error.contains("stopped again"), "{error}");
    }
}
