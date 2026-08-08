//! Turning the log back into state.
//!
//! Separate from the ledger because writing a log and deriving meaning from one are
//! different jobs. This half is pure, so it is tested without touching a disk.

use std::collections::BTreeMap;

use crate::{AgentId, Event, ProcessHandle, Record};

/// Agents the log says are still running.
///
/// This is the whole point of the log. Nothing else remembers what was running, so a
/// supervisor starting up asks the log rather than guessing.
///
/// Note the word says. This is a claim, not a fact. Confirming it against the machine is
/// `aos_supervisor::recover`, because a crash writes nothing and the log cannot know.
pub fn believed_running(records: &[Record]) -> BTreeMap<AgentId, ProcessHandle> {
    let mut live = BTreeMap::new();
    for record in records {
        // Matched variant by variant on purpose, with no catch-all. A catch-all treats every
        // new event as an ending, which is how `planned` would have silently marked a running
        // agent stopped. This way a new event type fails to compile until someone decides.
        match &record.event {
            Event::Started { handle, .. } => {
                live.insert(record.agent.clone(), *handle);
            }
            Event::Exited { .. }
            | Event::Stopped { .. }
            | Event::Refused { .. }
            | Event::LostWhileUnsupervised { .. } => {
                live.remove(&record.agent);
            }
            // An offer, not an outcome. Nothing about the machine changed.
            Event::Planned { .. } => {}
            // A capability call is something an agent did, not something that happened to
            // the agent. It may well have changed a file, and it does not change whether a
            // process is running, which is the only question this fold answers.
            Event::Capability { .. } => {}
        }
    }
    live
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(pid: u32) -> ProcessHandle {
        ProcessHandle {
            pid,
            start_token: pid as u64 * 7,
        }
    }

    fn started(pid: u32) -> Event {
        Event::Started {
            handle: handle(pid),
            program: "/usr/bin/sleep".into(),
        }
    }

    fn id(name: &str) -> AgentId {
        AgentId::new(name).unwrap()
    }

    fn record(seq: u64, agent: &str, event: Event) -> Record {
        Record {
            seq,
            at: seq,
            agent: id(agent),
            event,
        }
    }

    #[test]
    fn keeps_only_agents_that_never_ended() {
        let records = [
            record(1, "alive", started(10)),
            record(2, "gone", started(11)),
            record(3, "gone", Event::Exited { code: Some(0) }),
        ];
        let live = believed_running(&records);
        assert_eq!(live.keys().collect::<Vec<_>>(), vec![&id("alive")]);
        assert_eq!(live[&id("alive")], handle(10));
    }

    /// An agent restarted under the same name must be remembered by its newest handle,
    /// otherwise a stop would go to the pid of the previous run.
    #[test]
    fn a_restart_replaces_the_old_handle() {
        let records = [
            record(1, "loop", started(10)),
            record(2, "loop", Event::Exited { code: Some(1) }),
            record(3, "loop", started(20)),
        ];
        assert_eq!(believed_running(&records)[&id("loop")], handle(20));
    }

    #[test]
    fn a_refusal_never_marks_an_agent_running() {
        let records = [record(
            1,
            "blocked",
            Event::Refused {
                reason: "not allowed".into(),
            },
        )];
        assert!(believed_running(&records).is_empty());
    }

    /// A stop and a loss both end an agent, so neither may leave it in the live set.
    #[test]
    fn every_ending_event_clears_the_agent() {
        for ending in [
            Event::Exited { code: Some(0) },
            Event::Stopped { code: None },
            Event::LostWhileUnsupervised { handle: handle(10) },
        ] {
            let records = [record(1, "x", started(10)), record(2, "x", ending.clone())];
            assert!(
                believed_running(&records).is_empty(),
                "{ending:?} should clear the agent"
            );
        }
    }

    /// A plan is an offer. Recording one must not make a running agent look stopped, which
    /// is exactly what the old catch-all would have done.
    #[test]
    fn a_plan_does_not_disturb_a_running_agent() {
        let records = [
            record(1, "worker", started(10)),
            record(
                2,
                "worker",
                Event::Planned {
                    plan: crate::PlanId::from("abc".to_string()),
                    tier: crate::RiskTier::Destructive,
                },
            ),
        ];
        assert_eq!(believed_running(&records)[&id("worker")], handle(10));
    }

    #[test]
    fn an_empty_log_has_nothing_running() {
        assert!(believed_running(&[]).is_empty());
    }
}
