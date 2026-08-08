//! `aos`, the command line front end.
//!
//! `validate` and `status` read files and need nothing running. Everything else talks to
//! `aosd`, which is the only thing that owns agents. Two owners would mean two answers to
//! "what is running", and the whole point of the log is that there is one.

mod client;
mod commands;
mod runtime;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "aos", version, about = "Agent native layer over Linux")]
struct Cli {
    /// Where the event log, allowlist, agent output and socket live.
    #[arg(long, default_value = "run", global = true)]
    run_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a spec file without starting anything.
    Validate { spec: PathBuf },

    /// Replay the event log and report which agents are genuinely still running.
    ///
    /// Reads the log directly, so it works with no daemon running.
    Status,

    /// Start an agent and supervise it in the foreground until it exits.
    ///
    /// Standalone. Does not involve the daemon, and the daemon will not know about it.
    Run { spec: PathBuf },

    /// Ask the daemon what it is holding.
    Ping,

    /// List the agents the daemon is supervising.
    List,

    /// Ask the daemon to start an agent.
    ///
    /// Above tier read this only reports what would happen. Rerun it with the plan id it
    /// prints to actually go ahead.
    Start {
        spec: PathBuf,
        /// The plan id from a previous run of this exact command.
        #[arg(long)]
        commit: Option<String>,
    },

    /// Ask the daemon to stop an agent.
    Stop {
        agent: String,
        /// Seconds it gets to stop cleanly before it is killed.
        #[arg(long, default_value_t = 5)]
        grace: u64,
    },

    /// Kill switch. Stops every agent the daemon holds.
    StopAll {
        #[arg(long, default_value_t = 5)]
        grace: u64,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dir = &cli.run_dir;

    match cli.command {
        Command::Validate { spec } => commands::validate(&spec),
        Command::Status => runtime::status(dir),
        Command::Run { spec } => runtime::run(dir, &spec),
        Command::Ping => commands::ping(dir),
        Command::List => commands::list(dir),
        Command::Start { spec, commit } => commands::start(dir, &spec, commit),
        Command::Stop { agent, grace } => commands::stop(dir, &agent, grace),
        Command::StopAll { grace } => commands::stop_all(dir, grace),
    }
}

/// Reads the program allowlist. Used by the standalone `run` path.
pub fn allowlist(run_dir: &std::path::Path) -> Result<Vec<String>> {
    let path = run_dir.join("allowed-programs.json");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("no allowlist at {} ({e})", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("{} is not a JSON array of strings ({e})", path.display()))
}
