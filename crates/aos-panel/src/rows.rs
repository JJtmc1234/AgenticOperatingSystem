//! Folding the ledger into one row per agent.
//!
//! Pure, and deliberately the same shape as `aos_core::fold::believed_running`: matched
//! variant by variant with no catch-all, so a new event type fails to compile here until
//! somebody decides what it means to a row rather than being silently treated as an ending.
//!
//! This is a fold over the log and nothing else. It says what the log claims, which is not the
//! same as what the machine is doing. `aos_supervisor::recover` is the thing that checks a
//! claim against `/proc`, and the panel does not do that, so every word on screen is hedged
//! the way the log hedges it.

use aos_core::{AgentId, Event, Record, RiskTier};

/// Where an agent is, according to the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Life {
    Running {
        pid: u32,
        since: u64,
    },
    Ended {
        how: Ending,
        code: Option<i32>,
        at: u64,
    },
    /// The log mentions this agent but never says a process existed. A plan or a capability
    /// call with no start is exactly this, and it is not the same thing as stopped.
    NeverRan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Exited,
    Stopped,
    Refused,
    Lost,
}

impl Ending {
    pub const fn word(self) -> &'static str {
        match self {
            Ending::Exited => "EXITED",
            Ending::Stopped => "STOPPED",
            Ending::Refused => "REFUSED",
            Ending::Lost => "LOST",
        }
    }
}

impl Life {
    /// The all caps state word. Always a word, never colour on its own.
    pub const fn word(&self) -> &'static str {
        match self {
            Life::Running { .. } => "RUNNING",
            Life::Ended { how, .. } => how.word(),
            Life::NeverRan => "NEVER RAN",
        }
    }

    /// The line under the word, which is where the pid or the exit code goes.
    pub fn detail(&self) -> String {
        match self {
            Life::Running { pid, .. } => format!("pid {pid}"),
            Life::Ended {
                how: Ending::Refused,
                ..
            } => "nothing was launched".into(),
            Life::Ended {
                how: Ending::Lost, ..
            } => "vanished while nobody was supervising".into(),
            Life::Ended { code, .. } => format!("exit {}", crate::format::exit_code(*code)),
            Life::NeverRan => "no start was ever recorded".into(),
        }
    }

    pub const fn is_running(&self) -> bool {
        matches!(self, Life::Running { .. })
    }
}

/// One agent, as the log describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRow {
    pub id: AgentId,
    pub life: Life,
    /// The highest tier this agent has been recorded at.
    ///
    /// Not its ceiling. A ceiling lives in the spec and the log never carries one, so this is
    /// the highest tier the log has actually seen and `None` means the log has seen none.
    /// Calling it the ceiling would be inventing a fact.
    pub highest_tier: Option<RiskTier>,
    /// The newest record naming this agent, which is what the row's last column shows.
    pub last: Record,
    pub refusals: usize,
    pub records: usize,
}

/// One row per agent named anywhere in the log, newest activity first, running agents above
/// everything else.
///
/// Records are folded in the order given. The ledger is append only with a monotonic sequence,
/// so that order is the order things happened.
pub fn fold(records: &[Record]) -> Vec<AgentRow> {
    let mut rows: Vec<AgentRow> = Vec::new();

    for record in records {
        let at = record.at;
        let index = match rows.iter().position(|r| r.id == record.agent) {
            Some(i) => i,
            None => {
                rows.push(AgentRow {
                    id: record.agent.clone(),
                    life: Life::NeverRan,
                    highest_tier: None,
                    last: record.clone(),
                    refusals: 0,
                    records: 0,
                });
                rows.len() - 1
            }
        };
        let row = &mut rows[index];
        row.records += 1;
        row.last = record.clone();
        if crate::format::is_refusal(record) {
            row.refusals += 1;
        }

        // No catch-all. Adding an event type has to be a decision made here.
        match &record.event {
            Event::Started { handle, .. } => {
                row.life = Life::Running {
                    pid: handle.pid,
                    since: at,
                };
            }
            Event::Exited { code } => {
                row.life = Life::Ended {
                    how: Ending::Exited,
                    code: *code,
                    at,
                };
            }
            Event::Stopped { code } => {
                row.life = Life::Ended {
                    how: Ending::Stopped,
                    code: *code,
                    at,
                };
            }
            // A refusal is a launch that never happened, so it cannot end one that did.
            // `aos_core::fold` stopped treating it as an ending in bug 11, and two folds
            // disagreeing about what is running would be worse than either being wrong on
            // its own. The refusal is still counted above and still shown in the feed.
            //
            // An agent whose only history is a refusal has nothing else to say, so that row
            // still reads REFUSED rather than staying blank.
            Event::Refused { .. } => {
                if matches!(row.life, Life::NeverRan) {
                    row.life = Life::Ended {
                        how: Ending::Refused,
                        code: None,
                        at,
                    };
                }
            }
            Event::LostWhileUnsupervised { .. } => {
                row.life = Life::Ended {
                    how: Ending::Lost,
                    code: None,
                    at,
                };
            }
            // An offer, not an outcome. It carries a tier and changes nothing else.
            Event::Planned { tier, .. } => raise(&mut row.highest_tier, *tier),
            // Something the agent did, not something that happened to the agent. It cannot
            // make a stopped agent look running.
            Event::Capability { tier, .. } => raise(&mut row.highest_tier, *tier),
        }
    }

    rows.sort_by(|a, b| {
        b.life
            .is_running()
            .cmp(&a.life.is_running())
            .then(b.last.seq.cmp(&a.last.seq))
            .then(a.id.as_str().cmp(b.id.as_str()))
    });
    rows
}

fn raise(highest: &mut Option<RiskTier>, tier: RiskTier) {
    *highest = Some(match *highest {
        Some(seen) => seen.max(tier),
        None => tier,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_core::ProcessHandle;

    fn id(name: &str) -> AgentId {
        AgentId::new(name).unwrap()
    }

    fn record(seq: u64, agent: &str, event: Event) -> Record {
        Record {
            seq,
            at: 1_700_000_000 + seq,
            agent: id(agent),
            event,
        }
    }

    fn started(pid: u32) -> Event {
        Event::Started {
            handle: ProcessHandle {
                pid,
                start_token: pid as u64 * 7,
            },
            program: "/usr/bin/sleep".into(),
        }
    }

    #[test]
    fn an_empty_ledger_folds_to_no_rows() {
        assert!(fold(&[]).is_empty());
    }

    #[test]
    fn an_agent_that_started_is_running_with_its_pid() {
        let rows = fold(&[record(1, "brief", started(4242))]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id("brief"));
        assert_eq!(
            rows[0].life,
            Life::Running {
                pid: 4242,
                since: 1_700_000_001
            }
        );
        assert_eq!(rows[0].life.word(), "RUNNING");
        assert_eq!(rows[0].life.detail(), "pid 4242");
        assert_eq!(rows[0].records, 1);
    }

    /// The ordinary life of an agent, and the one thing the row must not do is stay running
    /// after the log says it ended.
    #[test]
    fn an_agent_that_started_then_exited_is_stopped_with_its_code() {
        let rows = fold(&[
            record(1, "brief", started(4242)),
            record(2, "brief", Event::Exited { code: Some(3) }),
        ]);
        assert_eq!(rows.len(), 1, "one agent, not one row per record");
        assert_eq!(
            rows[0].life,
            Life::Ended {
                how: Ending::Exited,
                code: Some(3),
                at: 1_700_000_002
            }
        );
        assert_eq!(rows[0].life.word(), "EXITED");
        assert_eq!(rows[0].life.detail(), "exit 3");
        assert_eq!(rows[0].records, 2);
    }

    /// A signal leaves no exit code. Drawing a zero there would say it succeeded.
    #[test]
    fn an_agent_killed_by_a_signal_says_its_code_is_unknown() {
        let rows = fold(&[
            record(1, "brief", started(1)),
            record(2, "brief", Event::Stopped { code: None }),
        ]);
        assert!(rows[0].life.detail().contains("unknown"));
    }

    /// A refusal must reach the row, not just the feed. An agent whose only history is a
    /// refusal is one somebody has to be able to find.
    #[test]
    fn a_refusal_is_surfaced_on_the_row_that_was_refused() {
        let rows = fold(&[record(
            1,
            "blocked",
            Event::Refused {
                reason: "program is not on the allowlist".into(),
            },
        )]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].refusals, 1);
        assert_eq!(rows[0].life.word(), "REFUSED");
        assert!(!rows[0].life.is_running(), "a refusal never runs anything");
        assert!(rows[0].life.detail().contains("nothing was launched"));
    }

    /// A refusal leaves a running agent running, exactly as `aos_core::fold` treats it. Two
    /// folds disagreeing about what is running would be worse than either being wrong.
    ///
    /// This test used to assert the opposite, because both folds used to end the agent. Bug
    /// 11 is what that cost: refusing a start because the agent was already running erased
    /// the live one, and nothing could then find or stop it. The core fold was fixed and this
    /// one was not, so the panel would have shown a running agent as REFUSED.
    #[test]
    fn a_refusal_leaves_a_running_agent_running_the_same_way_the_core_fold_does() {
        let records = [
            record(1, "brief", started(9)),
            record(
                2,
                "brief",
                Event::Refused {
                    reason: "brief is already running".into(),
                },
            ),
        ];
        assert!(
            fold(&records)[0].life.is_running(),
            "the refused second start must not end the first one"
        );
        assert_eq!(fold(&records)[0].refusals, 1, "and it is still counted");
        assert!(!aos_core::believed_running(&records).is_empty());
    }

    /// And a refusal after an ending must not revive anything, or the fix would have swapped
    /// one wrong answer for another. `aos_core::fold` has the same pair of tests.
    #[test]
    fn a_refusal_after_an_exit_leaves_the_agent_ended() {
        let records = [
            record(1, "brief", started(9)),
            record(2, "brief", Event::Exited { code: Some(0) }),
            record(
                3,
                "brief",
                Event::Refused {
                    reason: "no".into(),
                },
            ),
        ];
        let rows = fold(&records);
        assert!(!rows[0].life.is_running());
        assert_eq!(rows[0].life.word(), "EXITED", "not REFUSED");
        assert!(aos_core::believed_running(&records).is_empty());
    }

    /// A capability call is something the agent did. It must not disturb the process state.
    #[test]
    fn a_capability_call_does_not_change_whether_an_agent_is_running() {
        let rows = fold(&[
            record(1, "worker", started(11)),
            record(
                2,
                "worker",
                Event::Capability {
                    tool: "write_file".into(),
                    tier: RiskTier::Write,
                    outcome: "done".into(),
                },
            ),
        ]);
        assert!(rows[0].life.is_running());
        assert_eq!(rows[0].highest_tier, Some(RiskTier::Write));
    }

    /// The log never records a ceiling, so an agent nothing was gated for has no tier at all,
    /// and the row must say so rather than defaulting to the harmless one.
    #[test]
    fn an_agent_with_no_gated_call_has_no_known_tier() {
        let rows = fold(&[record(1, "brief", started(1))]);
        assert_eq!(rows[0].highest_tier, None);
    }

    #[test]
    fn the_highest_tier_seen_is_the_one_kept() {
        let rows = fold(&[
            record(
                1,
                "worker",
                Event::Capability {
                    tool: "read_file".into(),
                    tier: RiskTier::Read,
                    outcome: "done".into(),
                },
            ),
            record(
                2,
                "worker",
                Event::Planned {
                    plan: "abc".to_string().into(),
                    tier: RiskTier::Destructive,
                },
            ),
            record(
                3,
                "worker",
                Event::Capability {
                    tool: "read_file".into(),
                    tier: RiskTier::Read,
                    outcome: "done".into(),
                },
            ),
        ]);
        assert_eq!(rows[0].highest_tier, Some(RiskTier::Destructive));
    }

    #[test]
    fn an_agent_only_ever_planned_never_ran() {
        let rows = fold(&[record(
            1,
            "risky",
            Event::Planned {
                plan: "abc".to_string().into(),
                tier: RiskTier::Destructive,
            },
        )]);
        assert_eq!(rows[0].life, Life::NeverRan);
        assert_eq!(rows[0].life.word(), "NEVER RAN");
    }

    /// A restart under the same name has to be remembered by the newest pid, or a person
    /// reading the panel would go looking for a process that ended.
    #[test]
    fn a_restart_replaces_the_old_pid() {
        let rows = fold(&[
            record(1, "loop", started(10)),
            record(2, "loop", Event::Exited { code: Some(1) }),
            record(3, "loop", started(20)),
        ]);
        assert_eq!(
            rows[0].life,
            Life::Running {
                pid: 20,
                since: 1_700_000_003
            }
        );
    }

    #[test]
    fn running_agents_sort_above_finished_ones_and_the_newest_first() {
        let rows = fold(&[
            record(1, "old-runner", started(1)),
            record(2, "finished", started(2)),
            record(3, "finished", Event::Exited { code: Some(0) }),
            record(4, "new-runner", started(4)),
        ]);
        let order: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(order, vec!["new-runner", "old-runner", "finished"]);
    }
}
