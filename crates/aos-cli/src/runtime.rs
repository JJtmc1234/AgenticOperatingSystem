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
    let mut ledger = Ledger::open(log_path(run_dir))?;
    let sup = Supervisor::new(allowed, run_dir.join("logs"));

    // The gate, before anything is started. This path used to go straight to the supervisor,
    // so a policy denying every tier applied to `aos start` and was ignored by `aos run`,
    // which is the same machine and the same agents. Whether the rules applied came down to
    // which subcommand you happened to type.
    //
    // `decide_without_handshake` rather than `decide`, because this process starts an agent
    // and waits for it. A plan it offered would die with it, so there is no second call that
    // could quote one.
    if let aos_core::Decision::Denied { reason } =
        aos_core::Gate::open(run_dir)?.decide_without_handshake(&spec)
    {
        // Recorded before the error is returned. A refusal nobody wrote down is the half of
        // the log worth having, and a log that only records what worked hides exactly the
        // calls worth reviewing. The daemon already records its own.
        ledger.append(
            now(),
            spec.id.clone(),
            Event::Refused {
                reason: reason.clone(),
            },
        )?;
        anyhow::bail!("{reason}");
    }

    supervise(ledger, sup, spec)
}

/// Runs one agent to completion against a ledger and supervisor already built.
///
/// Split out from `run` so a test can hand it a log that refuses writes, which is the failure
/// the recovery below exists for and the one a real file will not produce on demand.
///
/// The gate is in `run` and not here. A denied agent must never reach a supervisor at all,
/// and having one is this function's entire premise.
fn supervise(mut ledger: Ledger, mut sup: Supervisor, spec: AgentSpec) -> Result<()> {
    // A start cannot be written before it happens. The pid and its start token do not exist
    // until the child does, so there is nothing truthful to append beforehand. The rule
    // instead is that a start which could not be recorded is undone, because a running agent
    // nobody wrote down is one nothing on this machine can find, stop or account for. `aosd`
    // does the same thing in `Daemon::launch`.
    let handle = match sup.start(&spec) {
        Ok(launched) => {
            let recorded = ledger.append(
                now(),
                spec.id.clone(),
                Event::Started {
                    handle: launched.handle,
                    // The file that ran, not the spelling asked for.
                    program: launched.program.display().to_string(),
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
                            launched.handle.pid
                        ),
                    }
                );
            }
            launched.handle
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

    // A sink that will not take a write will not put one on the device either. Never
    // reached in these tests, because the write fails first, but a Durable that quietly
    // says yes here would be a lie waiting for the next test to trip over.
    impl aos_core::ledger::Durable for Refusing {
        fn sync(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("no space left on device"))
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
        // Resolved, because the supervisor takes a resolved allowlist now rather than raw
        // strings, which is what stops a name on $PATH deciding which binary runs.
        let sup = Supervisor::new(
            aos_core::Allowlist::resolve(["/usr/bin/sleep".to_string()]).unwrap(),
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

    /// A run directory with a policy, an allowlist and one spec in it.
    fn run_dir(policy: &str, ceiling: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("policy.toml"), policy).unwrap();
        std::fs::write(
            dir.path().join("allowed-programs.json"),
            r#"["/usr/bin/sleep"]"#,
        )
        .unwrap();

        let spec = dir.path().join("spec.json");
        std::fs::write(
            &spec,
            format!(
                r#"{{"id":"hello","program":"/usr/bin/sleep","args":["0.01"],"ceiling":"{ceiling}"}}"#
            ),
        )
        .unwrap();
        (dir, spec)
    }

    const DENY_EVERYTHING: &str = r#"
plan_ttl_secs = 120

[tiers]
read = "deny"
write = "deny"
system = "deny"
destructive = "deny"
"#;

    const DEFAULT_ISH: &str = r#"
plan_ttl_secs = 120

[tiers]
read = "allow"
write = "prompt"
system = "prompt"
destructive = "prompt"
"#;

    /// The bug this guards. `run` went straight to the supervisor, so the policy applied to
    /// `aos start` and was ignored by `aos run`. Same machine, same agents, and whether the
    /// rules held came down to which subcommand somebody typed.
    #[test]
    fn a_denying_policy_refuses_aos_run() {
        let (dir, spec) = run_dir(DENY_EVERYTHING, "read");

        let outcome = run(dir.path(), &spec);

        let error = outcome
            .expect_err("a denied agent must not run")
            .to_string();
        assert!(error.contains("policy denies"), "{error}");

        // The refusal is in the log, because a refusal nobody wrote down is the half of the
        // record worth having.
        let records = aos_core::ledger::read(log_path(dir.path())).unwrap();
        assert_eq!(records.len(), 1, "{records:?}");
        assert!(
            matches!(records[0].event, Event::Refused { .. }),
            "{:?}",
            records[0]
        );
    }

    /// Above read there is no way to commit from here, so it refuses and says where to go
    /// rather than starting the agent or offering a plan that dies with this process.
    #[test]
    fn a_prompt_tier_refuses_aos_run_and_points_at_the_daemon() {
        let (dir, spec) = run_dir(DEFAULT_ISH, "destructive");

        let error = run(dir.path(), &spec)
            .expect_err("destructive must not run without a commit")
            .to_string();

        assert!(error.contains("needs a commit"), "{error}");
        assert!(error.contains("aos start"), "{error}");
    }

    /// The allowed case still works, or the fix would be a denial of service rather than a gate.
    #[test]
    fn an_allowed_agent_still_runs_to_completion() {
        let (dir, spec) = run_dir(DEFAULT_ISH, "read");

        run(dir.path(), &spec).expect("read is allowed and should run");

        let kinds: Vec<_> = aos_core::ledger::read(log_path(dir.path()))
            .unwrap()
            .into_iter()
            .map(|r| match r.event {
                Event::Started { .. } => "started",
                Event::Exited { .. } => "exited",
                Event::Refused { .. } => "refused",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, vec!["started", "exited"], "{kinds:?}");
    }
}
