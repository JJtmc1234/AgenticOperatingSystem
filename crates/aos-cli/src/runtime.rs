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
    let mut sup = Supervisor::new(allowed, run_dir.join("logs"));

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
        // the log worth having, and the daemon already records its own.
        ledger.append(
            now(),
            spec.id.clone(),
            Event::Refused {
                reason: reason.clone(),
            },
        )?;
        anyhow::bail!("{reason}");
    }

    // Append, then act. A refusal is written too, because a log that only records what
    // worked hides exactly the calls worth reviewing.
    let handle = match sup.start(&spec) {
        Ok(handle) => {
            ledger.append(
                now(),
                spec.id.clone(),
                Event::Started {
                    handle,
                    program: spec.program.clone(),
                },
            )?;
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
