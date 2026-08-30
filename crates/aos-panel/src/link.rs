//! Whether a daemon is answering, as a value rather than as an assumption.
//!
//! The panel has two independent sources and they fail independently. The ledger is a file and
//! keeps working with nothing running. The daemon is a socket and is the only thing that can
//! act. So "no daemon" is a normal state here, not an error state: the history is still true,
//! and the only thing that changes is that nothing can be commanded.
//!
//! `Unknown` is the starting value on purpose. Before the first ping nobody has asked, and
//! drawing that as connected or as down would both be inventing an answer.

/// What the last ping found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Link {
    /// Nobody has asked yet.
    #[default]
    Unknown,
    Answering {
        version: String,
        tracking: usize,
    },
    /// Asked and got nothing. `why` is the client's own words, which name the fix.
    Silent {
        why: String,
    },
}

impl Link {
    /// The all caps word, so the state reads without relying on the colour.
    pub const fn word(&self) -> &'static str {
        match self {
            Link::Unknown => "NOT ASKED",
            Link::Answering { .. } => "DAEMON UP",
            Link::Silent { .. } => "NO DAEMON",
        }
    }

    /// The sentence beside the word. Plain, because this is the one thing on screen that
    /// decides whether any of the buttons can do anything.
    pub fn sentence(&self) -> String {
        match self {
            Link::Unknown => "no ping has been answered or refused yet".into(),
            Link::Answering { version, tracking } => {
                format!("aosd {version}, supervising {tracking}")
            }
            Link::Silent { why } => {
                let first = why.lines().next().unwrap_or("no reason given");
                format!("{first}. The ledger below is still real history.")
            }
        }
    }

    pub const fn can_command(&self) -> bool {
        matches!(self, Link::Answering { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing may be commanded until a daemon has actually answered. Not knowing is not the
    /// same as knowing it is there.
    #[test]
    fn only_a_daemon_that_answered_can_be_commanded() {
        assert!(!Link::default().can_command());
        assert!(
            !Link::Silent {
                why: "no daemon answering on run/aosd.sock".into()
            }
            .can_command()
        );
        assert!(
            Link::Answering {
                version: "0.1.0".into(),
                tracking: 2
            }
            .can_command()
        );
    }

    #[test]
    fn every_link_state_has_its_own_words() {
        let states = [
            Link::Unknown,
            Link::Answering {
                version: "0.1.0".into(),
                tracking: 0,
            },
            Link::Silent {
                why: "connection refused".into(),
            },
        ];
        let mut words: Vec<&str> = states.iter().map(|s| s.word()).collect();
        words.sort_unstable();
        words.dedup();
        assert_eq!(words.len(), 3, "two link states share a word");
        for state in &states {
            assert!(!state.sentence().is_empty());
        }
    }

    /// A dead daemon must not read as a dead panel. The ledger is a file and keeps working.
    #[test]
    fn a_silent_daemon_says_the_ledger_is_still_good() {
        let why = "no daemon answering on run/aosd.sock (No such file or directory)";
        let said = Link::Silent { why: why.into() }.sentence();
        assert!(said.contains("no daemon answering"), "{said}");
        assert!(said.contains("still real history"), "{said}");
    }
}
