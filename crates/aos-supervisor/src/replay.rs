//! Rebuilding what is true from what the log claims.
//!
//! The log says which agents were running when it was last written. It cannot say whether
//! they still are, because a crash writes nothing. Replay is the step that reconciles the
//! claim against the machine.

use std::collections::BTreeMap;

use aos_core::{AgentId, ProcessHandle, Record, believed_running};

use crate::proc;

/// The outcome of reconciling the log against `/proc`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Recovered {
    /// Started, never ended, and the process is genuinely still the one we started.
    pub alive: BTreeMap<AgentId, ProcessHandle>,
    /// Started, never ended, and the process is gone. The log has a hole to close.
    pub lost: Vec<(AgentId, ProcessHandle)>,
}

/// Reconcile using the real `/proc`.
pub fn recover(records: &[Record]) -> Recovered {
    recover_with(records, proc::is_still)
}

/// Reconcile using any identity check.
///
/// Split out so the pid reuse case can be tested. Recycling a pid on demand is not something
/// a test can arrange, but lying about the check is.
pub fn recover_with(records: &[Record], is_still: impl Fn(ProcessHandle) -> bool) -> Recovered {
    let mut out = Recovered::default();
    for (agent, handle) in believed_running(records) {
        if is_still(handle) {
            out.alive.insert(agent, handle);
        } else {
            out.lost.push((agent, handle));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_core::Event;

    fn handle(pid: u32, start_token: u64) -> ProcessHandle {
        ProcessHandle { pid, start_token }
    }

    fn started(pid: u32, start_token: u64) -> Event {
        Event::Started {
            handle: handle(pid, start_token),
            program: "/usr/bin/sleep".into(),
        }
    }

    fn record(seq: u64, agent: &str, event: Event) -> Record {
        Record {
            seq,
            at: seq,
            agent: AgentId::new(agent).unwrap(),
            event,
        }
    }

    #[test]
    fn a_surviving_agent_is_alive() {
        let records = [record(1, "survivor", started(100, 55))];
        let out = recover_with(&records, |_| true);
        assert_eq!(out.alive.len(), 1);
        assert!(out.lost.is_empty());
    }

    #[test]
    fn an_agent_whose_process_is_gone_is_lost() {
        let records = [record(1, "goner", started(100, 55))];
        let out = recover_with(&records, |_| false);
        assert!(out.alive.is_empty());
        assert_eq!(
            out.lost,
            vec![(AgentId::new("goner").unwrap(), handle(100, 55))]
        );
    }

    /// The whole reason `start_token` exists. The pid is alive, but it belongs to something
    /// else now. Treating it as ours would mean signalling a stranger's process.
    #[test]
    fn a_recycled_pid_is_lost_rather_than_adopted() {
        let records = [record(1, "ours", started(100, 55))];

        // The pid exists, but its start time does not match what we recorded.
        let out = recover_with(&records, |h| proc_says(h, 100, 999));

        assert!(out.alive.is_empty(), "a recycled pid must never be adopted");
        assert_eq!(out.lost.len(), 1);
    }

    /// Stands in for `/proc`, reporting one pid with one start time.
    fn proc_says(asked: ProcessHandle, real_pid: u32, real_token: u64) -> bool {
        asked.pid == real_pid && asked.start_token == real_token
    }

    #[test]
    fn agents_that_already_ended_are_neither_alive_nor_lost() {
        let records = [
            record(1, "clean", started(100, 55)),
            record(2, "clean", Event::Exited { code: Some(0) }),
        ];
        assert_eq!(recover_with(&records, |_| true), Recovered::default());
    }

    #[test]
    fn an_empty_log_recovers_nothing() {
        assert_eq!(recover_with(&[], |_| true), Recovered::default());
    }
}
