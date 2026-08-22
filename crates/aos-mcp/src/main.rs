//! `aos-files`, a capability server Claude Code can connect to over MCP.
//!
//! Speaks on stdin and stdout, so it is started by the client rather than run as a service.
//! Everything it says that is not a reply goes to stderr, because stdout is the protocol.

use std::io::{BufReader, stdin, stdout};

use anyhow::{Context, Result};
use aos_core::{AgentId, Ledger, Policy};
use aos_mcp::{Root, Scope, Server};
use clap::Parser;

#[derive(Parser)]
#[command(about = "A file capability server for AOS, spoken over MCP")]
struct Args {
    /// The only directory this server may read. Must already exist.
    #[arg(long)]
    root: String,

    /// The one directory inside the root where changes may land. Must already exist.
    ///
    /// Left out, nothing may be changed at all. That is the safe default rather than an
    /// oversight: a capability nobody granted is a capability that is not held, and defaulting
    /// to the whole read root would make forgetting this flag look exactly like deciding to
    /// allow it.
    #[arg(long)]
    write_root: Option<String>,

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
    let scope = match &args.write_root {
        Some(w) => Scope::granting(root, w).context("the write scope is not usable")?,
        None => Scope::reading(root),
    };
    // Read before serving a single call. A capability server that starts with an unreadable
    // policy is a server enforcing nothing while looking perfectly healthy.
    let policy = Policy::load(&args.policy).context("the policy is not usable")?;
    let agent = AgentId::new(&args.agent).context("that is not a valid agent id")?;

    let ledger = match &args.log {
        Some(p) => Some(Ledger::open(p).context("the event log is not usable")?),
        None => None,
    };

    eprintln!(
        "aos-files: {}, agent {agent}, {} log",
        scope.describe(),
        if ledger.is_some() { "with a" } else { "no" }
    );

    let mut server = Server::new(scope, policy, agent, ledger);
    server.serve(BufReader::new(stdin().lock()), stdout().lock())?;
    Ok(())
}
