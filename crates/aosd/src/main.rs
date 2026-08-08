//! `aosd`, the long lived supervisor.
//!
//! One agent owner per run directory. The cli talks to it over a unix socket, so `list` and
//! `stop` finally mean the same thing from any terminal.
//!
//! Shutting the daemon down does not stop its agents. They keep running and the next boot
//! adopts them. A supervisor that killed everything whenever it restarted would make restarts
//! frightening, and a frightening restart is one nobody does.

mod daemon;
mod listen;
mod serve;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

pub use daemon::allowlist;

#[derive(Parser)]
#[command(name = "aosd", version, about = "The AOS agent supervisor daemon")]
struct Cli {
    /// Where the event log, allowlist, agent output and socket live.
    #[arg(long, default_value = "run")]
    run_dir: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    serve::run(&cli.run_dir)
}
