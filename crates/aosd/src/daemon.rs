//! The daemon's state and the one place requests are turned into actions.
//!
//! Losing a record for a process that is genuinely running is the failure that strands agents
//! nobody can find, so no append here is allowed to fail quietly.
//!
//! Starting and stopping cannot append first. A pid and its start token do not exist until
//! after the spawn, and an exit code does not exist until after the stop, so there is nothing
//! truthful to write down beforehand. The rule those two obey instead is that a mutation which
//! could not be recorded is either undone or reported as unrecorded, never dropped. `launch`
//! stops the process it could not write down. `stop` and `stop_all` report the agent as
//! stopped but unrecorded, because undoing a stop is not possible and pretending it failed
//! would be false.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use aos_core::{AgentReport, AgentSpec, Decision, Event, Gate, Ledger, PlanId, Request, Response};
use aos_supervisor::Supervisor;

pub struct Daemon {
    supervisor: Supervisor,
    ledger: Ledger,
    /// Policy and the plans outstanding against it. Shared with `aos run` through `aos-core`,
    /// so there is one implementation of what is allowed rather than one per entry point.
    gate: Gate,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

impl Daemon {
    /// Boots by replaying the log and taking back whatever is genuinely still running.
    pub fn boot(run_dir: &Path) -> Result<Self> {
        let allowed = crate::allowlist(run_dir)?;
        let log = run_dir.join("events.jsonl");

        let records = aos_core::ledger::read(&log)?;
        let mut ledger = Ledger::open(&log)?;
        let mut supervisor = Supervisor::new(allowed, run_dir.join("logs"));

        let gate = Gate::open(run_dir)?;

        let recovered = supervisor.adopt_from(&records);

        // Close the log's holes. An agent the log calls running that is not running leaves a
        // record saying so, rather than being quietly dropped. A gap nobody wrote down is a
        // gap nobody can investigate.
        for (agent, handle) in &recovered.lost {
            ledger.append(
                now(),
                agent.clone(),
                Event::LostWhileUnsupervised { handle: *handle },
            )?;
        }

        if !recovered.alive.is_empty() || !recovered.lost.is_empty() {
            eprintln!(
                "adopted {} agent(s), {} lost while unsupervised",
                recovered.alive.len(),
                recovered.lost.len()
            );
        }

        Ok(Self {
            supervisor,
            ledger,
            gate,
        })
    }

    pub fn socket_path(run_dir: &Path) -> PathBuf {
        run_dir.join("aosd.sock")
    }

    pub fn handle(&mut self, request: Request) -> Response {
        match request {
            Request::Ping => Response::Pong {
                version: env!("CARGO_PKG_VERSION").into(),
                tracking: self.supervisor.list().len(),
            },
            Request::List => self.list(),
            Request::Start { spec, commit } => self.start(*spec, commit),
            Request::Stop { agent, grace_secs } => {
                self.stop(&agent, Duration::from_secs(grace_secs))
            }
            Request::StopAll { grace_secs } => self.stop_all(Duration::from_secs(grace_secs)),
        }
    }

    /// Writes down every agent that has finished since the last check.
    ///
    /// Called on a timer by the accept loop rather than only when a client asks something.
    /// Two things went wrong without it. A finished child stayed a zombie until some request
    /// happened to arrive, and its exit was never recorded at all, so `believed_running` kept
    /// calling it live and the next boot wrote `lost_while_unsupervised` for an agent that had
    /// exited cleanly with code 0. An event that fires on every normal exit stops meaning
    /// anything. See bug 7.
    pub fn record_exits(&mut self) {
        for (id, code) in self.supervisor.reap_finished() {
            // There is no caller to hand this to. A background tick has nowhere to return an
            // error, and stopping the daemon over one would be worse than saying so, so it
            // goes to stderr, which under systemd is journald.
            if let Err(e) = self
                .ledger
                .append(now(), id.clone(), Event::Exited { code })
            {
                eprintln!(
                    "{id} exited and it could not be recorded, so the log now disagrees with the machine: {e}"
                );
            }
        }
    }

    fn list(&mut self) -> Response {
        let agents = self
            .supervisor
            .list()
            .into_iter()
            .map(|(id, state)| AgentReport {
                adopted: self.supervisor.is_adopted(&id),
                id,
                state,
            })
            .collect();
        Response::Agents { agents }
    }

    /// Turns the gate's decision into a response, and writes down what was decided.
    ///
    /// Returns `None` to mean "go ahead", or a response to send back instead. Written this
    /// way so the caller cannot forget to check: there is no path to `launch` that does not
    /// go through here first.
    ///
    /// Deciding moved to `aos_core::Gate` so `aos run` judges requests the same way. What is
    /// left here is the daemon's half: the wire shape of a refusal, and the record of it.
    fn gate(&mut self, spec: &AgentSpec, commit: Option<PlanId>) -> Option<Response> {
        match self.gate.decide(spec, commit, now()) {
            Decision::Allow => None,

            // issue-4 moved the deciding into aos_core::Gate. issue-2 made every
            // ledger append checked. Both hold here: the shape of the answer comes
            // from Decision, and no record is thrown away with a bare let _.
            Decision::Denied { reason } | Decision::CommitRefused { reason } => {
                Some(self.refuse(&spec.id, reason))
            }

            Decision::Planned {
                plan,
                tier,
                summary,
            } => {
                let recorded = self.ledger.append(
                    now(),
                    spec.id.clone(),
                    Event::Planned {
                        plan: plan.clone(),
                        tier,
                    },
                );
                if let Err(e) = recorded {
                    // Do not hand out an offer the log has no record of. The plan
                    // stays in memory unused: its id was never disclosed, so it
                    // cannot be quoted, and it expires with the rest.
                    return Some(Response::error(format!(
                        "could not record the plan for {}, so none was offered: {e}",
                        spec.id
                    )));
                }
                Some(Response::PlanRequired {
                    plan,
                    agent: spec.id.clone(),
                    tier,
                    summary,
                })
            }

            // Never judged, so there is nothing to record against the agent.
            Decision::CouldNotPlan { reason } => Some(Response::error(reason)),
        }
    }

    /// Records a refusal and builds the response that reports it.
    ///
    /// Returns the response rather than nothing, so writing the record and answering the
    /// caller cannot come apart. A refusal that was not written down is indistinguishable
    /// later from one that never happened, and the audit log is the whole point of refusing
    /// out loud, so the caller is told about both failures rather than only the first.
    fn refuse(&mut self, agent: &aos_core::AgentId, reason: String) -> Response {
        let recorded = self.ledger.append(
            now(),
            agent.clone(),
            Event::Refused {
                reason: reason.clone(),
            },
        );
        match recorded {
            Ok(_) => Response::error(reason),
            Err(e) => Response::error(format!(
                "{reason}. This refusal could not be recorded either, so the log has a hole: {e}"
            )),
        }
    }

    fn start(&mut self, spec: AgentSpec, commit: Option<PlanId>) -> Response {
        if let Some(refusal) = self.gate(&spec, commit) {
            return refusal;
        }
        self.launch(spec)
    }

    /// Actually starts it. Private, and only reachable through `start`, so the gate cannot
    /// be bypassed by a future caller.
    fn launch(&mut self, spec: AgentSpec) -> Response {
        match self.supervisor.start(&spec) {
            Ok(handle) => {
                let recorded = self.ledger.append(
                    now(),
                    spec.id.clone(),
                    Event::Started {
                        handle,
                        program: spec.program.clone(),
                    },
                );
                if let Err(e) = recorded {
                    // The process is running and we could not write it down, so nothing else
                    // will ever know about it. Stop it rather than leave it unaccounted for.
                    let _ = self.supervisor.stop(&spec.id, Duration::from_secs(5));
                    return Response::error(format!(
                        "started {} but could not record it, so it was stopped again: {e}",
                        spec.id
                    ));
                }
                Response::Started { handle }
            }
            Err(err) => self.refuse(&spec.id, err.to_string()),
        }
    }

    fn stop(&mut self, agent: &aos_core::AgentId, grace: Duration) -> Response {
        match self.supervisor.stop(agent, grace) {
            Ok(state) => {
                let code = match state {
                    aos_core::AgentState::Stopped { code } => code,
                    aos_core::AgentState::Running { .. } => None,
                };
                let recorded = self
                    .ledger
                    .append(now(), agent.clone(), Event::Stopped { code });
                if let Err(e) = recorded {
                    // The agent really did stop, so saying otherwise would be a lie. But the
                    // log still calls it running, and `believed_running` folds over the log,
                    // so the next boot will look for this pid. Report both.
                    return Response::error(format!(
                        "stopped {agent} but could not record it, so the log still calls it \
                         running and needs reconciling: {e}"
                    ));
                }
                Response::Stopped {
                    agent: agent.clone(),
                    state,
                }
            }
            Err(err) => Response::error(err),
        }
    }

    fn stop_all(&mut self, grace: Duration) -> Response {
        let mut stopped = Vec::new();
        let mut failed = Vec::new();

        for (id, outcome) in self.supervisor.stop_all(grace) {
            match outcome {
                Ok(state) => {
                    let code = match state {
                        aos_core::AgentState::Stopped { code } => code,
                        aos_core::AgentState::Running { .. } => None,
                    };
                    let recorded = self
                        .ledger
                        .append(now(), id.clone(), Event::Stopped { code });
                    if let Err(e) = recorded {
                        // Both facts go back. It is in `stopped` because it genuinely stopped,
                        // and in `failed` because the log does not say so and the operator is
                        // the only one who can close that gap.
                        failed.push(format!(
                            "{id}: stopped, but the record could not be written, so the log \
                             still calls it running: {e}"
                        ));
                    }
                    stopped.push(id);
                }
                Err(e) => failed.push(format!("{id}: {e}")),
            }
        }

        Response::StoppedAll { stopped, failed }
    }
}

/// Reads the program allowlist.
pub fn allowlist(run_dir: &Path) -> Result<Vec<String>> {
    let path = run_dir.join("allowed-programs.json");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("no allowlist at {}", path.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("{} is not a JSON array of strings", path.display()))
}

/// What the daemon does when the log will not take a write.
///
/// These build a `Daemon` by hand rather than going through `boot`, because the failure being
/// tested is a refusing log and `boot` insists on a real openable file. The socket, the
/// protocol and the restart are covered by `tests/daemon.rs` against the real binary.
#[cfg(test)]
mod tests {
    use super::*;
    use aos_core::{AgentId, Policy, RiskTier, Verdict};

    /// A sink that refuses every write, which is what a full disk looks like from here.
    struct Refusing;

    impl std::io::Write for Refusing {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("no space left on device"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    // A sink that will not take a write will not put one on the device either. Never
    // reached in these tests, because the write fails first, but a Durable that quietly
    // says yes here would be a lie waiting for the next test to trip over.
    impl aos_core::ledger::Durable for Refusing {
        fn sync(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("no space left on device"))
        }
    }

    fn spec(id: &str, program: &str, args: &[&str], ceiling: RiskTier) -> AgentSpec {
        AgentSpec {
            id: AgentId::new(id).unwrap(),
            program: program.into(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            ceiling,
        }
    }

    /// A daemon whose every append fails, over a real supervisor so processes are real.
    ///
    /// Policy and plans became one `Gate` so `aos run` judges requests the same way the
    /// daemon does, so this builds one rather than setting the two fields it replaced.
    fn refusing(policy: Policy, log_dir: &Path) -> Daemon {
        Daemon {
            supervisor: Supervisor::new(["/usr/bin/sleep".to_string()], log_dir.to_path_buf()),
            ledger: Ledger::to_sink(Box::new(Refusing), 1),
            gate: Gate::new(policy),
        }
    }

    fn message(response: Response) -> String {
        match response {
            Response::Error { message } => message,
            other => panic!("expected an error, got {other:?}"),
        }
    }

    /// The refusal reason and the failed write are both the caller's business. Reporting only
    /// the refusal is what the old code did, and it left the operator believing the log had
    /// recorded something it had not.
    #[test]
    fn a_refusal_that_could_not_be_written_reports_both() {
        let dir = tempfile::tempdir().unwrap();
        let mut policy = Policy::default();
        policy
            .agents
            .insert(AgentId::new("wiper").unwrap(), Verdict::Deny);

        let response = refusing(policy, dir.path()).handle(Request::Start {
            spec: Box::new(spec("wiper", "/usr/bin/sleep", &["1"], RiskTier::Read)),
            commit: None,
        });

        let message = message(response);
        assert!(message.contains("policy denies"), "{message}");
        assert!(message.contains("could not be recorded"), "{message}");
    }

    /// An offer the log has no record of is not an offer. Nothing has run at this point, so
    /// refusing to plan costs only a retry.
    #[test]
    fn a_plan_that_could_not_be_recorded_is_not_offered() {
        let dir = tempfile::tempdir().unwrap();

        // Write is Prompt under the default policy, so this is the planning call.
        let response = refusing(Policy::default(), dir.path()).handle(Request::Start {
            spec: Box::new(spec("risky", "/usr/bin/sleep", &["1"], RiskTier::Write)),
            commit: None,
        });

        let message = message(response);
        assert!(message.contains("could not record the plan"), "{message}");
        assert!(message.contains("none was offered"), "{message}");
    }

    /// The stop happened, so the response must not claim it failed. The log still calls the
    /// agent running though, and `believed_running` folds over the log, so the next boot would
    /// hunt a pid Linux may have handed to someone else. Both halves get reported.
    #[test]
    fn a_stop_that_could_not_be_recorded_is_reported_as_unrecorded() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = refusing(Policy::default(), dir.path());
        let spec = spec("sleeper", "/usr/bin/sleep", &["30"], RiskTier::Read);

        // Started directly on the supervisor, because a start through the daemon would hit the
        // same refusing log and be stopped again by `launch`.
        daemon.supervisor.start(&spec).unwrap();

        let response = daemon.handle(Request::Stop {
            agent: spec.id.clone(),
            grace_secs: 1,
        });

        let message = message(response);
        assert!(message.contains("stopped sleeper"), "{message}");
        assert!(message.contains("needs reconciling"), "{message}");

        // And it is genuinely gone, not merely reported.
        assert!(matches!(
            daemon.supervisor.state(&spec.id),
            Ok(aos_core::AgentState::Stopped { .. }) | Err(_)
        ));
    }

    /// The kill switch has somewhere to put both facts already, so it uses both. Leaving the
    /// agent out of `stopped` would be false, and leaving `failed` empty would hide the hole.
    #[test]
    fn stop_all_reports_an_agent_it_stopped_but_could_not_record() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = refusing(Policy::default(), dir.path());
        let spec = spec("sleeper", "/usr/bin/sleep", &["30"], RiskTier::Read);
        daemon.supervisor.start(&spec).unwrap();

        let response = daemon.handle(Request::StopAll { grace_secs: 1 });

        let Response::StoppedAll { stopped, failed } = response else {
            panic!("expected stop-all to answer, got {response:?}");
        };
        assert_eq!(stopped, vec![spec.id.clone()]);
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert!(failed[0].contains("could not be written"), "{failed:?}");
    }

    /// A daemon over a real run directory, so the ledger can be read back afterwards.
    fn daemon(dir: &Path) -> Daemon {
        let policy = Policy::default();
        Daemon {
            supervisor: Supervisor::new(
                ["/usr/bin/true".to_string(), "/usr/bin/sleep".to_string()],
                dir.join("logs"),
            ),
            ledger: Ledger::open(dir.join("events.jsonl")).unwrap(),
            // Policy and plans became one Gate, so this sets the field that replaced them.
            gate: Gate::new(policy),
        }
    }

    fn events(dir: &Path) -> Vec<String> {
        aos_core::ledger::read(dir.join("events.jsonl"))
            .unwrap()
            .into_iter()
            .map(|r| match r.event {
                Event::Started { .. } => "started".to_string(),
                Event::Exited { code } => format!("exited {code:?}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    /// The bug. Reaping only happened as a side effect of a `list` or `ping`, and no `Exited`
    /// record was ever written by the daemon at all. So a finished child sat as a zombie until
    /// somebody happened to ask, and the log kept believing it was running, which made the next
    /// boot write `lost_while_unsupervised` for an agent that had exited cleanly with code 0.
    #[test]
    fn an_agent_that_finished_is_recorded_without_anyone_asking() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = daemon(dir.path());

        // Through the real request path, so the `started` record is written the way it is in
        // production rather than by reaching past the gate.
        let started = daemon.handle(Request::Start {
            spec: Box::new(spec("quick", "/usr/bin/true", &[], RiskTier::Read)),
            commit: None,
        });
        assert!(matches!(started, Response::Started { .. }), "{started:?}");

        // No request of any kind, only the timer the accept loop drives.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            daemon.record_exits();
            if events(dir.path()).iter().any(|e| e.starts_with("exited")) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert_eq!(
            events(dir.path()),
            vec!["started".to_string(), "exited Some(0)".to_string()],
            "a clean exit has to reach the log without a client asking"
        );

        // And the log now agrees with the machine, which is what stops the next boot calling
        // this a loss.
        let records = aos_core::ledger::read(dir.path().join("events.jsonl")).unwrap();
        assert!(aos_core::believed_running(&records).is_empty());
    }

    /// A running agent must not be reaped or recorded as exited by the same timer.
    #[test]
    fn a_running_agent_is_left_alone_by_the_reaper() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = daemon(dir.path());
        let s = spec("slow", "/usr/bin/sleep", &["30"], RiskTier::Read);
        let started = daemon.handle(Request::Start {
            spec: Box::new(s.clone()),
            commit: None,
        });
        assert!(matches!(started, Response::Started { .. }), "{started:?}");

        for _ in 0..5 {
            daemon.record_exits();
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(events(dir.path()), vec!["started".to_string()]);

        let records = aos_core::ledger::read(dir.path().join("events.jsonl")).unwrap();
        assert_eq!(aos_core::believed_running(&records).len(), 1);

        let _ = daemon.supervisor.stop(&s.id, Duration::from_secs(5));
    }
}
