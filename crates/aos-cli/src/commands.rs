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
                println!("stopped  {}  exit {code:?}{origin}", report.id)
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
            // The run directory is in the line, always, not only when it differs from the
            // default. `--run-dir` is global and defaults to `run`, so on any other directory
            // the printed command targeted the wrong daemon, and this is the remedy the tool
            // itself hands you for the one flow with a deadline on it. The default plan ttl is
            // 120 seconds, so the round trip spent working out what was missing can expire the
            // plan and force the whole handshake again.
            //
            // Always rather than conditionally, because a line that is sometimes complete is a
            // line nobody can trust without reading it, and the conditional is one more thing
            // to get wrong. See bug 8.
            println!(
                "  aos --run-dir {} start {} --commit {plan}",
                absolute(run_dir).display(),
                absolute(spec_path).display()
            );
            // Not a success. Nothing was started, and a caller that reads only the exit status
            // could not tell this apart from the agent now running. See bug 7.
            Ok(Exit::NeedsAgreement)
        }
        other => bail!("unexpected answer to start: {other:?}"),
    }
}

/// A path as an absolute one, or unchanged if it cannot be resolved.
///
/// Both the run directory and the spec path go through this, and the second one is the reason
/// it exists rather than a `--run-dir` prefix on its own. The issue that asked for the run
/// directory wanted the printed line to be copy and paste safe. It was not, even with the run
/// directory added: `aos --run-dir /tmp/rd start examples/risky.json --commit ...` fails from
/// any other working directory, because the spec path is relative too. Fixing one and not the
/// other leaves the same failure reachable by walking to a different directory instead of
/// forgetting a flag. See bug 8.
///
/// Falls back to the path as given, which is the right answer for a path that will not resolve:
/// a line that is still wrong is better than one that is wrong and pretends otherwise.
fn absolute(path: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
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
