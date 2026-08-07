//! Loading specs and running one in the foreground.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use aos_core::{AgentSpec, AgentState, AuditEntry, AuditSink, JsonlSink, Outcome};
use aos_supervisor::Supervisor;

pub fn load_spec(path: &Path) -> Result<AgentSpec> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("{} is not a valid agent spec", path.display()))
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
    let mut audit = JsonlSink::open(run_dir.join("audit.jsonl"))?;
    let mut sup = Supervisor::new(allowed, run_dir.join("logs"));

    let note = |audit: &mut JsonlSink, outcome, reason: Option<String>| {
        let entry = AuditEntry {
            at: now(),
            agent: spec.id.clone(),
            action: "agent.start".into(),
            tier: spec.ceiling,
            outcome,
            reason,
        };
        audit.record(&entry)
    };

    // Every attempt writes exactly one entry, refusals included. A log that only records
    // successes hides precisely the calls worth reviewing.
    let started = match sup.start(&spec) {
        Ok(state) => {
            note(&mut audit, Outcome::Allowed, None)?;
            state
        }
        Err(err) => {
            note(&mut audit, Outcome::Refused, Some(err.to_string()))?;
            return Err(err.into());
        }
    };

    let AgentState::Running { pid } = started else {
        anyhow::bail!("{} exited before it could be supervised", spec.id);
    };
    println!(
        "{} running as pid {pid}, output at {}",
        spec.id,
        sup.log_path(&spec.id).display()
    );

    // Foreground supervision. Poll until it exits, then report how it went.
    loop {
        match sup.state(&spec.id)? {
            AgentState::Stopped { code } => {
                println!("{} stopped, exit code {code:?}", spec.id);
                return Ok(());
            }
            AgentState::Running { .. } => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}
