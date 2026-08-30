//! Taking what the worker sent back and turning it into what the screen says.
//!
//! Separate from the drawing and from the sending, because this is where the plan then commit
//! rule actually lives. The rule is not "the commit button is hidden", it is that the only
//! state carrying a plan id is one the daemon put the panel into by offering a plan.

use aos_core::{AgentId, PlanId, RiskTier};

use super::{App, Note, StartFlow};
use crate::worker::{Order, Outcome, Tone};

impl App {
    /// One thing the worker sent. Applied on the UI thread, where nothing blocks.
    pub fn apply(&mut self, outcome: Outcome) {
        // Only an answer to something a person asked clears what they are waiting on. The
        // worker's own ping must never make an outstanding stop look finished.
        let asked = if outcome.settles_an_order() {
            self.in_flight.take()
        } else {
            None
        };

        match outcome {
            Outcome::Heartbeat(link) => self.link = link,
            Outcome::SpecLoaded { spec, verdict } => {
                self.start = StartFlow::Loaded { spec, verdict };
            }
            Outcome::PlanOffered {
                plan,
                agent,
                tier,
                summary,
            } => self.accept_plan(plan, agent, tier, summary),
            Outcome::Note { text, tone } => {
                self.note(&text, tone);
                self.settle_start(asked.as_ref());
            }
        }
    }

    /// A plan only becomes committable if the panel asked for it and it is for the spec it
    /// asked about.
    ///
    /// Both checks matter. An offer arriving in any other state is one this panel never asked
    /// for, and an offer naming a different agent would let a plan for something harmless
    /// commit something else, which is exactly what the handshake exists to prevent.
    fn accept_plan(&mut self, plan: PlanId, agent: AgentId, tier: RiskTier, summary: String) {
        let StartFlow::Planning { spec } = &self.start else {
            self.note(
                "a plan arrived that this panel never asked for, so it was dropped",
                Tone::Bad,
            );
            return;
        };

        if spec.id != agent {
            self.note(
                &format!(
                    "the daemon offered a plan for {agent} but {} was asked about, so it was \
                     dropped",
                    spec.id
                ),
                Tone::Bad,
            );
            self.start = StartFlow::Idle;
            return;
        }

        self.start = StartFlow::Offered {
            spec: spec.clone(),
            plan,
            agent,
            tier,
            summary,
        };
    }

    /// A start that ended in a message rather than in a plan is over.
    ///
    /// A refused commit resets to the beginning rather than leaving the button armed. The plan
    /// the daemon refused is spent or wrong, and a second click on the same id would only be
    /// refused again while looking like a retry that might work.
    fn settle_start(&mut self, asked: Option<&Order>) {
        match asked {
            Some(Order::Plan(_)) if matches!(self.start, StartFlow::Planning { .. }) => {
                self.start = StartFlow::Idle;
            }
            Some(Order::Commit { .. }) => self.start = StartFlow::Idle,
            _ => {}
        }
    }

    /// Adds a line to the short list of what just happened, newest first.
    pub fn note(&mut self, text: &str, tone: Tone) {
        self.notes.insert(
            0,
            Note {
                at: self.now,
                text: text.to_string(),
                tone,
            },
        );
        self.notes.truncate(super::NOTES_KEPT);
    }
}
