//! Risk tiers, carried over from the Windows AOS.
//!
//! A tier says how much damage a call could do, not what it does. Policy maps a tier to a
//! verdict. The ordering matters, so `Destructive > System > Write > Read` is derived rather
//! than written by hand, which is why the variants are declared in that order.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskTier {
    /// Observes the machine and changes nothing.
    Read,
    /// Changes data the user owns and could restore.
    Write,
    /// Changes machine state outside the user's own files.
    System,
    /// Loses something that cannot be brought back.
    Destructive,
}

impl RiskTier {
    /// Whether a call at this tier may run without an explicit commit.
    ///
    /// Read is the only tier that may. Everything above it plans first, so a mistaken plan
    /// costs nothing and a human sees the change before it lands.
    pub const fn allows_implicit_commit(self) -> bool {
        matches!(self, RiskTier::Read)
    }
}

impl std::fmt::Display for RiskTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            RiskTier::Read => "read",
            RiskTier::Write => "write",
            RiskTier::System => "system",
            RiskTier::Destructive => "destructive",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_escalate_in_declaration_order() {
        assert!(RiskTier::Read < RiskTier::Write);
        assert!(RiskTier::Write < RiskTier::System);
        assert!(RiskTier::System < RiskTier::Destructive);
    }

    /// Harness invariant. Only Read may skip the commit handshake. This test exists so that
    /// adding a tier later fails loudly rather than quietly granting it a free pass.
    #[test]
    fn only_read_runs_without_a_commit() {
        let all = [
            RiskTier::Read,
            RiskTier::Write,
            RiskTier::System,
            RiskTier::Destructive,
        ];
        for tier in all {
            assert_eq!(
                tier.allows_implicit_commit(),
                tier == RiskTier::Read,
                "{tier} has the wrong commit rule"
            );
        }
    }

    #[test]
    fn serde_round_trip_is_lowercase() {
        let json = serde_json::to_string(&RiskTier::Destructive).unwrap();
        assert_eq!(json, "\"destructive\"");
        assert_eq!(
            serde_json::from_str::<RiskTier>(&json).unwrap(),
            RiskTier::Destructive
        );
    }
}
