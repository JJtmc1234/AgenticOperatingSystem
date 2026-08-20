//! One function per subcommand, so `main` stays a table of contents.

use std::path::Path;

use anyhow::{Result, bail};
use aos_core::{AgentId, AgentState, PlanId, Request, Response};

use crate::Exit;
use crate::client;
use crate::runtime;

pub fn validate(path: &Path) -> Result<()> {
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

pub fn ping(run_dir: &Path) -> Result<()> {
    match client::demand(run_dir, &Request::Ping)? {
        Response::Pong { version, tracking } => {
            println!("aosd {version} is up, supervising {tracking} agent(s)");
            Ok(())
        }
        other => bail!("unexpected answer to ping: {other:?}"),
    }
}

pub fn list(run_dir: &Path) -> Result<()> {
    let Response::Agents { agents } = client::demand(run_dir, &Request::List)? else {
        bail!("unexpected answer to list");
    };

    if agents.is_empty() {
        println!("no agents");
        return Ok(());
    }

    for report in agents {
        // Adopted agents are marked, because their exit code will never be knowable and the
        // reader should not wonder why it is missing later.
        let origin = if report.adopted { "  (adopted)" } else { "" };
        match report.state {
            AgentState::Running { pid } => println!("running  {}  pid {pid}{origin}", report.id),
            AgentState::Stopped { code } => {
                println!(
                    "stopped  {}  {}{origin}",
                    report.id,
                    runtime::describe(code)
                )
            }
        }
    }
    Ok(())
}

pub fn start(run_dir: &Path, spec_path: &Path, commit: Option<String>) -> Result<Exit> {
    let spec = runtime::load_spec(spec_path)?;
    let id = spec.id.clone();

    let request = Request::Start {
        spec: Box::new(spec),
        commit: commit.map(PlanId::from),
    };

    match client::demand(run_dir, &request)? {
        Response::Started { handle } => {
            println!("{id} started as pid {}", handle.pid);
            Ok(Exit::Acted)
        }
        // Nothing happened yet. Print what would, and how to say yes.
        Response::PlanRequired {
            plan,
            tier,
            summary,
            ..
        } => {
            println!("tier {tier} needs a commit, so nothing has run.");
            println!();
            println!("  {summary}");
            println!();
            println!("To go ahead:");
            println!("  aos start {} --commit {plan}", spec_path.display());
            // Not a success. Nothing was started, and a caller that reads only the exit status
            // could not tell this apart from the agent now running. See bug 7.
            Ok(Exit::NeedsAgreement)
        }
        other => bail!("unexpected answer to start: {other:?}"),
    }
}

pub fn stop(run_dir: &Path, agent: &str, grace: u64) -> Result<()> {
    let request = Request::Stop {
        agent: AgentId::new(agent)?,
        grace_secs: grace,
    };
    let Response::Stopped { agent, state } = client::demand(run_dir, &request)? else {
        bail!("unexpected answer to stop");
    };

    match state {
        AgentState::Stopped { code: Some(code) } => println!("{agent} stopped, exit {code}"),
        // No code means it was signalled, or it was adopted and init took the code.
        AgentState::Stopped { code: None } => println!("{agent} stopped"),
        AgentState::Running { pid } => println!("{agent} is somehow still running as pid {pid}"),
    }
    Ok(())
}

pub fn stop_all(run_dir: &Path, grace: u64) -> Result<Exit> {
    let request = Request::StopAll { grace_secs: grace };
    let Response::StoppedAll { stopped, failed } = client::demand(run_dir, &request)? else {
        bail!("unexpected answer to stop-all");
    };

    if stopped.is_empty() && failed.is_empty() {
        println!("nothing was running");
        return Ok(Exit::Acted);
    }

    for id in &stopped {
        println!("stopped  {id}");
    }
    for problem in &failed {
        println!("FAILED   {problem}");
    }

    // A partial kill switch is a failure, not a success. Exiting zero here would let a script
    // carry on believing the machine is quiet.
    if !failed.is_empty() {
        // Deliberately a failure rather than the "needs agreement" code. Some agents did not
        // stop, which is worse than nothing having happened: the machine is not quiet and the
        // caller cannot fix it by agreeing to anything.
        bail!("{} agent(s) did not stop", failed.len());
    }
    Ok(Exit::Acted)
}
