//! What a run with no human in front of it prints.
//!
//! `--frames` and `--seconds` exist so the panel can be checked by something that cannot look
//! at a screen. A frame count on its own only proves the process did not crash, so the line it
//! prints carries the state the frames were drawn from: what the ledger held, what the fold
//! made of it, and whether a daemon answered. That turns "it opened" into something with a
//! claim in it that can be wrong.

use crate::app::App;

/// One line describing what is on screen.
pub fn summary(app: &App, drawn: u32) -> String {
    format!(
        "drew {drawn} frames. link {}, ledger {}, {} records, {} agents, {} running, \
         {} refused, seq {}, ledger read {}",
        app.link.word(),
        if app.ledger.exists {
            "present"
        } else {
            "not there"
        },
        app.records().len(),
        app.rows().len(),
        app.running(),
        app.refusals(),
        app.last_seq(),
        match app.ledger.read_at {
            Some(at) => crate::format::ago(app.now, at),
            None => "never".into(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line has to carry a claim about the data, not just a frame count, or a run against
    /// an empty directory would print the same thing as a run against a real ledger.
    #[test]
    fn the_summary_reports_what_was_read_and_not_only_that_it_drew() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(dir.path().to_path_buf());
        app.tick();
        let line = summary(&app, 12);
        assert!(line.contains("drew 12 frames"), "{line}");
        assert!(line.contains("link NOT ASKED"), "{line}");
        assert!(line.contains("ledger not there"), "{line}");
        assert!(line.contains("0 records"), "{line}");
    }
}
