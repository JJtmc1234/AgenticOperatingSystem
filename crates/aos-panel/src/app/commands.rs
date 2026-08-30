//! The things a person can ask for.
//!
//! Every one of them goes out through the worker and comes back as an outcome. None of them
//! change what is on screen by themselves, because the panel showing an agent as stopped when
//! the stop actually failed would put it and the daemon into two different versions of the
//! truth, and the panel would be the one that was wrong.

use std::path::PathBuf;

use aos_core::AgentId;

use super::{App, StartFlow};
use crate::worker::{Order, Tone};

impl App {
    /// The one place an order is sent. Everything that commands the daemon goes through here,
    /// so "nothing is sent without the screen saying it was sent" is structural rather than
    /// something every call site has to remember.
    fn dispatch(&mut self, order: Order) {
        if self.in_flight.is_some() {
            return;
        }
        if self.worker.send(order.clone()) {
            self.in_flight = Some(order);
        } else {
            self.note("the worker thread has gone, so nothing was sent", Tone::Bad);
        }
    }

    pub fn ping(&mut self) {
        self.dispatch(Order::Ping);
    }

    pub fn stop(&mut self, agent: AgentId) {
        let grace = self.grace_secs;
        self.dispatch(Order::Stop {
            agent,
            grace_secs: grace,
        });
    }

    pub fn stop_all(&mut self) {
        let grace = self.grace_secs;
        self.dispatch(Order::StopAll { grace_secs: grace });
    }

    /// Reads a spec off disk. Sends nothing to the daemon and starts nothing.
    pub fn load_spec(&mut self) {
        let path = self.spec_path.trim().to_string();
        if path.is_empty() {
            self.note("no spec file was named", Tone::Bad);
            return;
        }
        self.start = StartFlow::Idle;
        self.dispatch(Order::LoadSpec(PathBuf::from(path)));
    }

    /// The planning call: a start with no commit quoted. Above whatever the policy allows
    /// outright, this runs nothing and comes back with a plan.
    pub fn request_plan(&mut self) {
        let StartFlow::Loaded { spec, .. } = &self.start else {
            return;
        };
        let spec = spec.clone();
        self.start = StartFlow::Planning { spec: spec.clone() };
        self.dispatch(Order::Plan(spec));
    }

    /// The committing call, and the only one. Reachable from no state except a plan the daemon
    /// actually offered, quoting that plan's id and the spec it was offered for.
    pub fn commit(&mut self) {
        let StartFlow::Offered { spec, plan, .. } = &self.start else {
            return;
        };
        let (spec, plan) = (spec.clone(), plan.clone());
        self.dispatch(Order::Commit { spec, plan });
    }

    /// Drops a plan without committing it. The plan expires in the daemon on its own.
    pub fn abandon_start(&mut self) {
        self.start = StartFlow::Idle;
    }
}
