//! Sampled diagnostics and supervised processes, separate from journal-backed tasks.
use crate::army::runtime::{Roll, Runtime};
use crate::providers::{Diagnostics, Snapshot as Diagnosed};

pub struct Facts {
    pub diagnostics: Diagnosed,
    pub runtime: Vec<Runtime>,
}

impl Facts {
    pub fn army_only() -> Self {
        Self {
            diagnostics: Diagnosed {
                army: Vec::new(),
                machine: Vec::new(),
            },
            runtime: Vec::new(),
        }
    }

    /// Reads supervisor evidence without creating runtime state.
    pub fn with_runtime(mut self, home: &std::path::Path) -> Self {
        self.runtime = Roll::open(home)
            .map(|roll| roll.all().cloned().collect())
            .unwrap_or_default();
        self
    }

    pub fn gather(diagnostics: &mut Diagnostics) -> Self {
        Self::gather_at(diagnostics, crate::army::event::now())
    }

    /// Diagnostics owns the sampling interval so refreshes cannot invent fresh measurements.
    pub fn gather_at(diagnostics: &mut Diagnostics, at: u64) -> Self {
        Self {
            diagnostics: diagnostics.snapshot_at(at),
            runtime: Vec::new(),
        }
    }
}
