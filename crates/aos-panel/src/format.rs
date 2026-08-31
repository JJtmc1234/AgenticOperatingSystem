//! Turning records into the words on the screen.
//!
//! Pure, so every line the panel can draw is checkable without a window. Nothing here decides
//! anything, it only says how a thing is written down.
//!
//! Two rules run through all of it. **A gap is drawn as a gap**, so an exit code that does not
//! exist says so rather than showing a plausible zero. And **every state has a word**, because
//! a red row is invisible to a colour blind reader and to a black and white screenshot.

use aos_core::{Event, Record};

/// How much a line wants to be noticed. Deliberately coarse: four steps is enough to paint
/// with and few enough that every one of them means something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// Something was refused. The interesting half of an audit log.
    Refusal,
    /// Worth a look but nobody was stopped.
    Notable,
    /// It happened and it worked.
    Ordinary,
    /// Nobody knows. Never guessed at.
    Unknown,
}

/// The all caps word for an event, which is what a person scans down the feed for.
pub const fn kind(event: &Event) -> &'static str {
    match event {
        Event::Started { .. } => "STARTED",
        Event::Exited { .. } => "EXITED",
        Event::Stopped { .. } => "STOPPED",
        Event::Refused { .. } => "REFUSED",
        Event::Planned { .. } => "PLANNED",
        Event::Capability { .. } => "CAPABILITY",
        Event::LostWhileUnsupervised { .. } => "LOST",
    }
}

/// The rest of the line: what actually happened, in the fewest true words.
pub fn detail(event: &Event) -> String {
    match event {
        Event::Started { handle, program } => format!("pid {} {program}", handle.pid),
        Event::Exited { code } => format!("exit {}", exit_code(*code)),
        Event::Stopped { code } => format!("exit {}", exit_code(*code)),
        Event::Refused { reason } => reason.clone(),
        Event::Planned { plan, tier } => format!("tier {tier}, plan {plan}"),
        Event::Capability {
            tool,
            tier,
            outcome,
        } => format!("{tool} at tier {tier}, {outcome}"),
        Event::LostWhileUnsupervised { handle } => format!(
            "pid {} was gone when the log was replayed, so its exit code is unknowable",
            handle.pid
        ),
    }
}

/// An exit code, or the reason there is not one.
///
/// `None` means the process was killed by a signal and never returned a code. Writing that as
/// `0` would say it succeeded, which is the opposite of what happened.
pub fn exit_code(code: Option<i32>) -> String {
    match code {
        Some(c) => c.to_string(),
        None => "unknown, killed by a signal".into(),
    }
}

/// How loudly to draw an event.
pub fn weight(event: &Event) -> Weight {
    match event {
        Event::Refused { .. } => Weight::Refusal,
        Event::LostWhileUnsupervised { .. } => Weight::Notable,
        Event::Planned { .. } => Weight::Notable,
        Event::Capability { outcome, .. } => capability_weight(outcome),
        Event::Started { .. } => Weight::Ordinary,
        Event::Exited { code } | Event::Stopped { code } => match code {
            Some(0) => Weight::Ordinary,
            Some(_) => Weight::Notable,
            None => Weight::Unknown,
        },
    }
}

/// A capability outcome is a free string written by the capability server, so this reads it
/// rather than matching on a type it does not have.
///
/// Anything unrecognised is `Unknown` on purpose. Guessing that an unfamiliar outcome went
/// well is exactly the mistake that would hide the next kind of refusal somebody adds.
fn capability_weight(outcome: &str) -> Weight {
    let o = outcome.trim().to_ascii_lowercase();
    if o.starts_with("refused") || o.starts_with("denied") || o.starts_with("no such") {
        Weight::Refusal
    } else if o.starts_with("failed") || o.starts_with("planned") {
        // A failure and an offer are both worth a look and neither was a refusal.
        Weight::Notable
    } else if o == "done" {
        Weight::Ordinary
    } else {
        Weight::Unknown
    }
}

/// Whether a record is one somebody would go looking for. Used to count refusals in the
/// header, so the interesting half of the log is visible without scrolling to it.
pub fn is_refusal(record: &Record) -> bool {
    weight(&record.event) == Weight::Refusal
}

/// How long ago, in the shortest form that is still true.
pub fn ago(now: u64, then: u64) -> String {
    // A record from the future means the clock moved, not that time ran backwards. Saying so
    // is better than showing a huge number or silently clamping to "now".
    if then > now.saturating_add(1) {
        return "ahead of this clock".into();
    }
    let d = now.saturating_sub(then);
    match d {
        0..=1 => "now".into(),
        2..=59 => format!("{d}s ago"),
        60..=3599 => format!("{}m ago", d / 60),
        3600..=86_399 => format!("{}h ago", d / 3600),
        _ => format!("{}d ago", d / 86_400),
    }
}

/// The wall clock time of a record, as UTC.
///
/// UTC rather than local, and labelled UTC everywhere it is drawn. Local time needs a zone
/// database, and a panel that shows an unlabelled time an hour out is worse than one that
/// shows a labelled time somebody has to add an hour to.
pub fn clock(unix_secs: u64) -> String {
    let s = unix_secs % 86_400;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_core::{AgentId, ProcessHandle, RiskTier};

    fn handle() -> ProcessHandle {
        ProcessHandle {
            pid: 4242,
            start_token: 991,
            // The panel only ever reads a handle, so which boot it came from does not matter here.
            boot: None,
        }
    }

    #[test]
    fn every_event_has_a_word_and_a_detail() {
        let events = [
            Event::Started {
                handle: handle(),
                program: "/usr/bin/echo".into(),
            },
            Event::Exited { code: Some(0) },
            Event::Stopped { code: None },
            Event::Refused {
                reason: "policy denies it".into(),
            },
            Event::Planned {
                plan: "abc".to_string().into(),
                tier: RiskTier::Destructive,
            },
            Event::Capability {
                tool: "write_file".into(),
                tier: RiskTier::Write,
                outcome: "done".into(),
            },
            Event::LostWhileUnsupervised { handle: handle() },
        ];
        for event in events {
            assert!(!kind(&event).is_empty(), "{event:?} has no word");
            assert!(!detail(&event).is_empty(), "{event:?} has no detail");
        }
    }

    /// A process killed by a signal has no exit code, and a zero there would read as success.
    #[test]
    fn a_missing_exit_code_is_drawn_as_missing() {
        assert_eq!(exit_code(Some(0)), "0");
        assert_eq!(exit_code(Some(137)), "137");
        assert!(exit_code(None).contains("unknown"));
        assert!(!exit_code(None).contains('0'));
    }

    #[test]
    fn a_refusal_outweighs_everything_ordinary() {
        assert_eq!(
            weight(&Event::Refused {
                reason: "no".into()
            }),
            Weight::Refusal
        );
        assert_eq!(
            weight(&Event::Started {
                handle: handle(),
                program: "x".into()
            }),
            Weight::Ordinary
        );
        assert_eq!(weight(&Event::Exited { code: Some(0) }), Weight::Ordinary);
        assert_eq!(weight(&Event::Exited { code: Some(1) }), Weight::Notable);
        assert_eq!(weight(&Event::Exited { code: None }), Weight::Unknown);
    }

    /// The capability server writes its outcome as prose, so the reading of it is pinned here.
    /// An unfamiliar outcome must come out unknown rather than quietly counted as a success.
    #[test]
    fn a_refused_capability_is_read_as_a_refusal() {
        let cap = |outcome: &str| {
            weight(&Event::Capability {
                tool: "delete_file".into(),
                tier: RiskTier::Destructive,
                outcome: outcome.into(),
            })
        };
        assert_eq!(cap("refused: outside the granted scope"), Weight::Refusal);
        assert_eq!(cap("denied by policy"), Weight::Refusal);
        assert_eq!(cap("no such capability"), Weight::Refusal);
        assert_eq!(cap("failed: permission denied"), Weight::Notable);
        assert_eq!(cap("planned 4f2a"), Weight::Notable);
        assert_eq!(cap("done"), Weight::Ordinary);
        assert_eq!(cap("something nobody has written yet"), Weight::Unknown);
    }

    #[test]
    fn refusals_are_countable() {
        let record = |event| Record {
            seq: 1,
            at: 10,
            agent: AgentId::new("a").unwrap(),
            event,
        };
        assert!(is_refusal(&record(Event::Refused {
            reason: "no".into()
        })));
        assert!(!is_refusal(&record(Event::Exited { code: Some(0) })));
    }

    #[test]
    fn ago_says_the_shortest_true_thing() {
        assert_eq!(ago(100, 100), "now");
        assert_eq!(ago(101, 100), "now");
        assert_eq!(ago(130, 100), "30s ago");
        assert_eq!(ago(1_000, 100), "15m ago");
        assert_eq!(ago(10_000, 100), "2h ago");
        assert_eq!(ago(100_000, 100), "1d ago");
    }

    /// A record stamped in the future means a clock moved. Saying that is more use than
    /// drawing "now" and pretending the two agree.
    #[test]
    fn a_record_from_the_future_is_called_out() {
        assert_eq!(ago(100, 500), "ahead of this clock");
    }

    #[test]
    fn the_clock_is_utc_and_zero_padded() {
        assert_eq!(clock(0), "00:00:00");
        assert_eq!(clock(3_661), "01:01:01");
        // 2026-08-20T09:14:07Z, checked against `date -u -d @1787217247`.
        assert_eq!(clock(1_787_217_247), "09:14:07");
    }
}
