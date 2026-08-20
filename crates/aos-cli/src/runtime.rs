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

pub fn run(run_dir: &Path, spec_path: &Path) -> Result<crate::Exit> {
    let spec = load_spec(spec_path)?;
    let allowed = crate::allowlist(run_dir)?;
    let mut ledger = Ledger::open(log_path(run_dir))?;
    let mut sup = Supervisor::new(allowed, run_dir.join("logs"));

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
                // Not `{code:?}`. That is Rust `Debug` of an `Option`, so a reader looking for
                // the number got the literal text `Some(1)` and a script had to parse it back
                // out of debug syntax. See bug 9.
                println!("{} stopped, {}", spec.id, describe(code));
                return Ok(exit_for(code));
            }
            AgentState::Running { .. } => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// How an agent's ending reads to a person.
pub fn describe(code: Option<i32>) -> String {
    match code {
        Some(0) => "exit code 0".to_string(),
        Some(code) => format!("exit code {code}"),
        // No number to give, because the kernel ended it rather than the program choosing to
        // stop. Saying so beats printing `None` and leaving somebody to work out what that was.
        None => "ended by a signal".to_string(),
    }
}

/// The status `aos run` should exit with, given what the agent did.
///
/// A signal death becomes 128, which is the base of the shell convention of 128 plus the signal
/// number. The number itself is not available here: `AgentState::Stopped` carries an
/// `Option<i32>` that is already `None` by the time it arrives, so 128 says "a signal ended it"
/// and does not pretend to say which.
fn exit_for(code: Option<i32>) -> crate::Exit {
    match code {
        Some(code) => crate::Exit::Agent(u8::try_from(code).unwrap_or(1)),
        None => crate::Exit::Agent(128),
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
