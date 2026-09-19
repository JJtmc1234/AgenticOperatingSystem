//! Workflow observations travel outside the army journal sequence, like telemetry.
use super::wire::{Frame, Reply};
use crate::providers::army::workflow;
use crate::providers::health::Diagnostic;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct Poll {
    checked: Option<Instant>,
    last: Option<Diagnostic>,
}

impl Poll {
    pub(super) fn next(&mut self, home: &Path) -> Option<Frame> {
        if self
            .checked
            .is_some_and(|at| at.elapsed() < Duration::from_secs(2))
        {
            return None;
        }
        self.checked = Some(Instant::now());
        let fresh = workflow::read(home);
        if self.last.is_none() && !home.join("evan/config.json").exists() {
            self.last = Some(fresh);
            return None;
        }
        if self.last.as_ref() == Some(&fresh) {
            return None;
        }
        self.last = Some(fresh.clone());
        Some(Frame::to(
            None,
            Reply::Telemetry {
                at: crate::army::event::now(),
                diagnostics: vec![fresh],
            },
        ))
    }
}
