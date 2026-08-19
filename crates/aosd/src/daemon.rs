//! The daemon's state and the one place requests are turned into actions.
//!
//! Every mutation appends to the log before the supervisor is told, so a crash can never
//! leave the log claiming less than actually happened. Losing a record for a process that is
//! genuinely running is the failure that strands agents nobody can find.

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

            Decision::Denied { reason } | Decision::CommitRefused { reason } => {
                self.record_refusal(&spec.id, &reason);
                Some(Response::error(reason))
            }

            Decision::Planned {
                plan,
                tier,
                summary,
            } => {
                let _ = self.ledger.append(
                    now(),
                    spec.id.clone(),
                    Event::Planned {
                        plan: plan.clone(),
                        tier,
                    },
                );
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

    fn record_refusal(&mut self, agent: &aos_core::AgentId, reason: &str) {
        let _ = self.ledger.append(
            now(),
            agent.clone(),
            Event::Refused {
                reason: reason.to_string(),
            },
        );
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
            Err(err) => {
                let reason = err.to_string();
                self.record_refusal(&spec.id, &reason);
                Response::error(reason)
            }
        }
    }

    fn stop(&mut self, agent: &aos_core::AgentId, grace: Duration) -> Response {
        match self.supervisor.stop(agent, grace) {
            Ok(state) => {
                let code = match state {
                    aos_core::AgentState::Stopped { code } => code,
                    aos_core::AgentState::Running { .. } => None,
                };
                let _ = self
                    .ledger
                    .append(now(), agent.clone(), Event::Stopped { code });
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
                    let _ = self
                        .ledger
                        .append(now(), id.clone(), Event::Stopped { code });
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
