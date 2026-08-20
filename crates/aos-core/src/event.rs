//! What happened, as data.
//!
//! The event log is the only durable state in AOS. Everything the supervisor believes is a
//! fold over these records, so a record is written before the belief changes, never after.
//! The shape of that rule is borrowed from Hunter's kernel: append, fold, then tell anyone
//! who is listening, in that order, always.

use serde::{Deserialize, Serialize};

use crate::AgentId;

/// A pid is not an identity. Linux reuses pids, so a record naming only a pid cannot tell
/// "my agent" apart from "whatever took its number afterwards".
///
/// `start_token` is the process start time in clock ticks since boot, field 22 of
/// `/proc/<pid>/stat`. The kernel assigns it once and never changes it, so the pair is
/// unique for as long as the machine has been up.
///
/// For as long as the machine has been up is the whole of it, and the log outlives that. The
/// run directory survives a reboot, so a record written before a power loss can be compared
/// against a machine that has since restarted, where the same pair means nothing. Early boot
/// is the dangerous part rather than a remote one: on the machine this was written on, 87
/// processes share start token 18 and 14 share token 19, because the startup sequence is
/// mostly deterministic and mostly runs in the same few ticks. So the boot is part of the
/// identity. See bug 8.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessHandle {
    pub pid: u32,
    pub start_token: u64,
    /// Which boot this pid and token belong to, from `/proc/sys/kernel/random/boot_id`.
    ///
    /// Optional so a log written before this field existed still reads. `None` means the boot
    /// is unknown rather than known to match, and an unknown boot is not adopted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// An agent was launched and is believed running.
    Started {
        #[serde(flatten)]
        handle: ProcessHandle,
        program: String,
    },
    /// The agent ended on its own.
    Exited { code: Option<i32> },
    /// The supervisor ended it.
    Stopped { code: Option<i32> },
    /// The launch was refused before anything ran.
    Refused { reason: String },
    /// Policy asked for a commit, so a plan was offered and nothing ran.
    Planned {
        plan: crate::PlanId,
        tier: crate::RiskTier,
    },
    /// A capability server was asked to do something, and this is what came of it.
    ///
    /// Refusals are recorded as well as successes, and the refusals are the interesting half.
    /// A log that only holds what happened cannot answer what an agent tried to do and was
    /// stopped from doing, which is the question you ask when something has gone wrong.
    Capability {
        tool: String,
        tier: crate::RiskTier,
        outcome: String,
    },

    /// Replay found a `Started` with no ending, and the process is gone.
    ///
    /// Written during replay so the gap is visible in the log rather than quietly patched.
    /// An agent that vanished while nobody was supervising is worth being able to find.
    LostWhileUnsupervised { handle: ProcessHandle },
}

impl Event {
    /// Whether this event means the agent is running afterwards.
    pub const fn leaves_agent_running(&self) -> bool {
        matches!(self, Event::Started { .. })
    }
}

/// One line of the log. `seq` orders records, `at` is unix seconds and is for humans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub seq: u64,
    pub at: u64,
    pub agent: AgentId,
    #[serde(flatten)]
    pub event: Event,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle() -> ProcessHandle {
        ProcessHandle {
            pid: 4242,
            start_token: 9_219_785,
            boot: None,
        }
    }

    #[test]
    fn a_record_round_trips_through_json() {
        let record = Record {
            seq: 1,
            at: 1_786_126_629,
            agent: AgentId::new("hello").unwrap(),
            event: Event::Started {
                handle: handle(),
                program: "/usr/bin/echo".into(),
            },
        };
        let line = serde_json::to_string(&record).unwrap();
        assert_eq!(serde_json::from_str::<Record>(&line).unwrap(), record);
    }

    /// The tag is what a human reads when scanning the log, so it is worth pinning.
    #[test]
    fn events_are_tagged_by_name() {
        let line = serde_json::to_string(&Event::Exited { code: Some(0) }).unwrap();
        assert_eq!(line, r#"{"event":"exited","code":0}"#);
    }

    #[test]
    fn only_started_leaves_an_agent_running() {
        assert!(
            Event::Started {
                handle: handle(),
                program: "/usr/bin/echo".into()
            }
            .leaves_agent_running()
        );

        assert!(
            !Event::Planned {
                plan: crate::PlanId::from("abc".to_string()),
                tier: crate::RiskTier::Destructive
            }
            .leaves_agent_running()
        );

        for ended in [
            Event::Exited { code: Some(0) },
            Event::Stopped { code: None },
            Event::Refused {
                reason: "nope".into(),
            },
            Event::LostWhileUnsupervised { handle: handle() },
        ] {
            assert!(
                !ended.leaves_agent_running(),
                "{ended:?} should end the agent"
            );
        }
    }
}
