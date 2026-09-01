//! The calls themselves: what each order does and what it makes of the answer.
//!
//! Split from the channel plumbing next door because these are two different jobs. This half
//! is the only place in the panel that knows there is a socket at all.

use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};

use aos_cli::client;
use aos_core::{AgentId, AgentSpec, Request, Response};

use super::{HEARTBEAT, Order, Outcome};
use crate::link::Link;

pub(super) fn serve(run_dir: &Path, orders: &Receiver<Order>, replies: &Sender<Outcome>) {
    // Ask once immediately. Waiting a whole heartbeat first would leave the panel saying NOT
    // ASKED for two seconds at every start, which is the moment somebody is actually looking
    // at it to find out whether anything is running.
    if replies.send(Outcome::Heartbeat(ping(run_dir))).is_err() {
        return;
    }

    loop {
        let batch = match orders.recv_timeout(HEARTBEAT) {
            Ok(order) => run(run_dir, order),
            // Idle. Ask whether the daemon is still there, so the screen keeps saying
            // something that was true two seconds ago rather than something from startup.
            Err(RecvTimeoutError::Timeout) => vec![Outcome::Heartbeat(ping(run_dir))],
            // The panel is gone.
            Err(RecvTimeoutError::Disconnected) => return,
        };
        for outcome in batch {
            if replies.send(outcome).is_err() {
                return;
            }
        }
    }
}

pub(super) fn run(run_dir: &Path, order: Order) -> Vec<Outcome> {
    match order {
        Order::Ping => vec![Outcome::Heartbeat(ping(run_dir))],

        Order::LoadSpec(path) => vec![load_spec(run_dir, &path)],

        Order::Stop { agent, grace_secs } => {
            let request = Request::Stop {
                agent: agent.clone(),
                grace_secs,
            };
            with_link(run_dir, request, move |response| match response {
                Response::Stopped { agent, state } => Outcome::good(format!(
                    "stopped {agent}, {}",
                    match state {
                        aos_core::AgentState::Stopped { code } =>
                            format!("exit {}", crate::format::exit_code(code)),
                        aos_core::AgentState::Running { pid } =>
                            format!("but it is still running as pid {pid}"),
                    }
                )),
                other => unexpected(other),
            })
        }

        Order::StopAll { grace_secs } => with_link(
            run_dir,
            Request::StopAll { grace_secs },
            |response| match response {
                Response::StoppedAll { stopped, failed } if failed.is_empty() => {
                    Outcome::good(format!("stopped {}", listed(&stopped)))
                }
                Response::StoppedAll { stopped, failed } => Outcome::bad(format!(
                    "stopped {}, and could not stop {}",
                    listed(&stopped),
                    failed.join(", ")
                )),
                other => unexpected(other),
            },
        ),

        Order::Plan(spec) => {
            let request = Request::Start {
                spec: spec.clone(),
                commit: None,
            };
            with_link(run_dir, request, |response| match response {
                Response::PlanRequired {
                    plan,
                    agent,
                    tier,
                    summary,
                } => Outcome::PlanOffered {
                    plan,
                    agent,
                    tier,
                    summary,
                },
                // The policy allowed this outright, so the daemon started it rather than
                // offering anything. Said plainly, because a person who expected a plan has
                // to find out that something is now running.
                Response::Started { handle } => Outcome::good(format!(
                    "the policy allowed this outright, so it is already running as pid {}. \
                     No plan was offered",
                    handle.pid
                )),
                other => unexpected(other),
            })
        }

        Order::Commit { spec, plan } => {
            let request = Request::Start {
                spec,
                commit: Some(plan),
            };
            with_link(run_dir, request, |response| match response {
                Response::Started { handle } => {
                    Outcome::good(format!("committed, running as pid {}", handle.pid))
                }
                other => unexpected(other),
            })
        }
    }
}

/// Sends one request, turns the answer into an outcome, and reports what the round trip said
/// about the connection either way.
///
/// The link is refreshed after every command rather than only on the heartbeat, so a panel
/// that just successfully stopped an agent cannot still be showing NO DAEMON.
fn with_link(
    run_dir: &Path,
    request: Request,
    settle: impl FnOnce(Response) -> Outcome,
) -> Vec<Outcome> {
    match client::ask(run_dir, &request) {
        Ok(Response::Error { message }) => vec![
            Outcome::bad(format!("refused: {message}")),
            Outcome::Heartbeat(ping(run_dir)),
        ],
        Ok(response) => vec![settle(response), Outcome::Heartbeat(ping(run_dir))],
        Err(e) => vec![
            Outcome::bad(one_line(&e.to_string())),
            Outcome::Heartbeat(Link::Silent { why: e.to_string() }),
        ],
    }
}

pub(super) fn ping(run_dir: &Path) -> Link {
    match client::ask(run_dir, &Request::Ping) {
        Ok(Response::Pong { version, tracking }) => Link::Answering { version, tracking },
        // Something answered and it was not a daemon speaking this protocol. That is not the
        // same as nothing being there, and it is not something to command either.
        Ok(other) => Link::Silent {
            why: format!("something answered the ping with {other:?}"),
        },
        Err(e) => Link::Silent { why: e.to_string() },
    }
}

/// Reads a spec and asks the policy on disk what starting it would take.
///
/// Read only, and it never touches the daemon. A missing policy file is not an error: the
/// daemon treats that as the safe default, so the panel says the verdict is not known here
/// rather than guessing the default and being wrong when a policy appears later.
pub(super) fn load_spec(run_dir: &Path, path: &Path) -> Outcome {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => return Outcome::bad(format!("cannot read {}: {e}", path.display())),
    };
    let spec: AgentSpec = match serde_json::from_str(&text) {
        Ok(spec) => spec,
        Err(e) => {
            return Outcome::bad(format!("{} is not a valid agent spec: {e}", path.display()));
        }
    };

    let policy_path = run_dir.join("policy.toml");
    let verdict = match (policy_path.exists(), aos_core::Policy::load(&policy_path)) {
        // The tier the daemon will judge at, which comes from the program. Labelling the
        // button from `spec.ceiling` would show a verdict for a tier nothing uses. See bug 34.
        (true, Ok(policy)) => {
            Some(policy.verdict(&spec.id, aos_core::program::tier_of(&spec.program)))
        }
        // A policy that will not parse is not a policy. Saying nothing is known beats
        // labelling the button from a file the daemon will refuse to load.
        _ => None,
    };

    Outcome::SpecLoaded {
        spec: Box::new(spec),
        verdict,
    }
}

pub(super) fn listed(agents: &[AgentId]) -> String {
    if agents.is_empty() {
        return "nothing, because nothing was running".into();
    }
    agents
        .iter()
        .map(AgentId::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A response that does not belong to the request that was sent.
///
/// Never quietly ignored. A daemon answering a stop with a plan means the two sides disagree
/// about the protocol, and that is worth putting on the screen.
fn unexpected(response: Response) -> Outcome {
    Outcome::bad(format!(
        "the daemon answered with {response:?}, which does not fit the request"
    ))
}

fn one_line(text: &str) -> String {
    text.lines().next().unwrap_or(text).to_string()
}
