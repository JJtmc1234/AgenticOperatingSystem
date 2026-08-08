//! `aos-files`, a capability server Claude Code can connect to over MCP.
//!
//! Speaks on stdin and stdout, so it is started by the client rather than run as a service.
//! Everything it says that is not a reply goes to stderr, because stdout is the protocol.

use std::io::{BufReader, stdin, stdout};

use anyhow::{Context, Result};
use aos_core::{AgentId, Ledger, Policy};
use aos_mcp::{Root, Server};
use clap::Parser;

#[derive(Parser)]
#[command(about = "A file capability server for AOS, spoken over MCP")]
struct Args {
    /// The only directory this server may touch. Must already exist.
    #[arg(long)]
    root: String,

    /// The policy file deciding what is allowed. See examples/policy.toml.
    #[arg(long)]
    policy: String,

    /// Which agent this is answering for, so the policy can single it out.
    #[arg(long, default_value = "claude-code")]
    agent: String,

    /// The event log to append gated calls to. Omitted means nothing is recorded.
    #[arg(long)]
    log: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let root = Root::open(&args.root).context("the root is not usable")?;
    // Read before serving a single call. A capability server that starts with an unreadable
    // policy is a server enforcing nothing while looking perfectly healthy.
    let policy = Policy::load(&args.policy).context("the policy is not usable")?;
    let agent = AgentId::new(&args.agent).context("that is not a valid agent id")?;

    let ledger = match &args.log {
        Some(p) => Some(Ledger::open(p).context("the event log is not usable")?),
        None => None,
    };

    eprintln!(
        "aos-files: root {}, agent {agent}, {} log",
        root.path().display(),
        if ledger.is_some() { "with a" } else { "no" }
    );

    let mut server = Server::new(root, policy, agent, ledger);
    server.serve(BufReader::new(stdin().lock()), stdout().lock())?;
    Ok(())
}
