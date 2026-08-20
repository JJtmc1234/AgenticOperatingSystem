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
    /// Started, never ended, and `/proc` would not say which. Neither adopted nor written off.
    ///
    /// Its own list because the two wrong answers are both bad in different directions.
    /// Adopting it would mean signalling a pid nothing has confirmed. Calling it lost writes a
    /// `lost_while_unsupervised` record, and `believed_running` folds over that, so the agent
    /// is dropped from every later boot as well: unreachable by `stop`, and unreachable by
    /// `stop-all`, which is the kill switch. An unreadable `/proc` used to land in `lost`, so
    /// an agent could escape the kill switch by renaming itself to something that is not UTF-8.
    /// See bug 7.
    pub unknown: Vec<(AgentId, ProcessHandle)>,
}

/// Reconcile using the real `/proc`.
pub fn recover(records: &[Record]) -> Recovered {
    recover_with(records, |handle| match proc::started(handle.pid) {
        proc::Started::At(token) if token == handle.start_token => Verdict::Ours,
        // Read fine and it is a different process, so the pid was recycled. Gone, as far as
        // our agent is concerned.
        proc::Started::At(_) | proc::Started::Gone => Verdict::Gone,
        proc::Started::Unknown => Verdict::CannotTell,
    })
}

/// What the identity check decided about one recorded pid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Still the process we started.
    Ours,
    /// Not there, or there and somebody else's.
    Gone,
    /// Could not find out. Not the same as gone.
    CannotTell,
}

/// Reconcile using any identity check.
///
/// Split out so the pid reuse case can be tested. Recycling a pid on demand is not something
/// a test can arrange, but lying about the check is.
pub fn recover_with(records: &[Record], check: impl Fn(ProcessHandle) -> Verdict) -> Recovered {
    let mut out = Recovered::default();
    for (agent, handle) in believed_running(records) {
        match check(handle) {
            Verdict::Ours => {
                out.alive.insert(agent, handle);
            }
            Verdict::Gone => out.lost.push((agent, handle)),
            Verdict::CannotTell => out.unknown.push((agent, handle)),
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
        let out = recover_with(&records, |_| Verdict::Ours);
        assert_eq!(out.alive.len(), 1);
        assert!(out.lost.is_empty());
    }

    #[test]
    fn an_agent_whose_process_is_gone_is_lost() {
        let records = [record(1, "goner", started(100, 55))];
        let out = recover_with(&records, |_| Verdict::Gone);
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
    fn proc_says(asked: ProcessHandle, real_pid: u32, real_token: u64) -> Verdict {
        if asked.pid == real_pid && asked.start_token == real_token {
            Verdict::Ours
        } else {
            Verdict::Gone
        }
    }

    /// The bug, at the level it does its damage. An agent `/proc` would not answer about used
    /// to be filed under `lost`, and `lost` is what boot writes a `lost_while_unsupervised`
    /// record for. `believed_running` folds over that record, so the agent is dropped from
    /// every later boot too, never adopted, and out of reach of `stop` and of `stop-all`.
    ///
    /// Not knowing has to be its own answer, because both of the other two are actively wrong.
    #[test]
    fn an_agent_that_cannot_be_checked_is_neither_adopted_nor_written_off() {
        let records = [record(1, "worker", started(100, 55))];
        let out = recover_with(&records, |_| Verdict::CannotTell);

        assert!(
            out.alive.is_empty(),
            "signalling a pid nothing confirmed is the other wrong answer"
        );
        assert!(
            out.lost.is_empty(),
            "writing it off is what put a live agent beyond the kill switch"
        );
        assert_eq!(out.unknown.len(), 1);
        assert_eq!(out.unknown[0].0, AgentId::new("worker").unwrap());
    }

    /// A pid that is genuinely gone is still lost, which is the case the log needs closed and
    /// the one that must not be swallowed by the change above.
    #[test]
    fn a_gone_agent_is_still_lost_rather_than_unknown() {
        let records = [record(1, "worker", started(100, 55))];
        let out = recover_with(&records, |_| Verdict::Gone);

        assert_eq!(out.lost.len(), 1);
        assert!(out.unknown.is_empty());
    }

    #[test]
    fn agents_that_already_ended_are_neither_alive_nor_lost() {
        let records = [
            record(1, "clean", started(100, 55)),
            record(2, "clean", Event::Exited { code: Some(0) }),
        ];
        assert_eq!(
            recover_with(&records, |_| Verdict::Ours),
            Recovered::default()
        );
    }

    #[test]
    fn an_empty_log_recovers_nothing() {
        assert_eq!(recover_with(&[], |_| Verdict::Ours), Recovered::default());
    }
}
