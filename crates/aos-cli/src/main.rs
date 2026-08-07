//! `aos`, the command line front end.
//!
//! Phase 0 supervises in the foreground only. Each invocation is its own process, so it
//! cannot see agents an earlier invocation started. Cross invocation `list` and `stop` need
//! the daemon, which is phase 1. Saying so here is cheaper than a command that half works.

mod runtime;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "aos", version, about = "Agent native layer over Linux")]
struct Cli {
    /// Where the audit log and runtime state live.
    #[arg(long, default_value = "run", global = true)]
    run_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a spec file without starting anything.
    Validate { spec: PathBuf },

    /// Start an agent and supervise it in the foreground until it exits.
    Run { spec: PathBuf },

    /// Replay the event log and report which agents are genuinely still running.
    Status,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { spec } => validate(&spec),
        Command::Run { spec } => runtime::run(&cli.run_dir, &spec),
        Command::Status => runtime::status(&cli.run_dir),
    }
}

fn validate(path: &Path) -> Result<()> {
    let spec = runtime::load_spec(path)?;
    println!(
        "ok   {} runs {} at tier {}",
        spec.id, spec.program, spec.ceiling
    );
    if !spec.args.is_empty() {
        println!("     args {:?}", spec.args);
    }
    Ok(())
}

/// Reads the program allowlist. Kept next to the spec loader so both fail the same way.
pub fn allowlist(run_dir: &Path) -> Result<Vec<String>> {
    let path = run_dir.join("allowed-programs.json");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("no allowlist at {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("{} is not a JSON array of strings", path.display()))
}
