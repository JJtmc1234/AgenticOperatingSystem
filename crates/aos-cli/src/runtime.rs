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

    // Append, then act. A refusal is written too, because a log that only records what
    // worked hides exactly the calls worth reviewing.
    let handle = match sup.start(&spec) {
        Ok(launched) => {
            ledger.append(
                now(),
                spec.id.clone(),
                Event::Started {
                    handle: launched.handle,
                    // The file that ran, not the spelling asked for.
                    program: launched.program.display().to_string(),
                },
            )?;
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
