//! Plan then commit.
//!
//! Nothing above tier Read happens in one step. The first call returns a plan describing what
//! would happen. A second call quotes the plan's id to make it happen.
//!
//! The point is not the two steps. It is that the plan records exactly what was agreed, so
//! planning something harmless and committing something dangerous is not possible.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{AgentId, AgentSpec, Error, Result, RiskTier};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlanId(String);

impl PlanId {
    /// 16 random bytes from the kernel, as hex.
    ///
    /// Unguessable rather than sequential. The socket is owner only, so this is a second
    /// lock, but a predictable id would let anything that could reach the socket commit a
    /// plan it never saw.
    pub fn fresh() -> Result<Self> {
        let mut bytes = [0u8; 16];
        // /dev/urandom rather than a crate. One dependency fewer, and it is the same source
        // the crates read from.
        use std::io::Read;
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;

        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        Ok(Self(hex))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PlanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for PlanId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// What was agreed to, and enough of it to prove nothing changed since.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub id: PlanId,
    pub agent: AgentId,
    pub tier: RiskTier,
    /// The exact spec, canonicalised. Compared whole on commit.
    ///
    /// The whole thing rather than a hash of it. A spec is a few hundred bytes, so there is
    /// nothing to save by hashing, and comparing the real text cannot collide.
    pub spec_json: String,
    pub created_at: u64,
}

impl Plan {
    fn canonical(spec: &AgentSpec) -> Result<String> {
        Ok(serde_json::to_string(spec)?)
    }
}

/// Holds plans until they are committed, expire, or the daemon restarts.
///
/// Deliberately not written to the event log. A plan is an offer, not a fact about the
/// machine, and an offer that survived a restart would let someone commit something the
/// current daemon never proposed.
#[derive(Debug, Default)]
pub struct PlanLedger {
    plans: BTreeMap<PlanId, Plan>,
    ttl_secs: u64,
}

impl PlanLedger {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            plans: BTreeMap::new(),
            ttl_secs,
        }
    }

    /// Records what would happen and returns the plan.
    pub fn propose(&mut self, spec: &AgentSpec, tier: RiskTier, now: u64) -> Result<Plan> {
        self.expire(now);

        let plan = Plan {
            id: PlanId::fresh()?,
            agent: spec.id.clone(),
            tier,
            spec_json: Plan::canonical(spec)?,
            created_at: now,
        };
        self.plans.insert(plan.id.clone(), plan.clone());
        Ok(plan)
    }

    /// Redeems a plan against the spec actually being committed.
    ///
    /// Four ways to fail, and each one is a real attack or a real mistake.
    pub fn commit(&mut self, id: &PlanId, spec: &AgentSpec, now: u64) -> Result<Plan> {
        self.expire(now);

        let plan = self
            .plans
            .get(id)
            .ok_or_else(|| Error::Refused(format!("no plan {id}, it may have expired")))?
            .clone();

        // The one that matters. Plan something harmless, commit something dangerous.
        if plan.spec_json != Plan::canonical(spec)? {
            return Err(Error::Refused(format!(
                "plan {id} was made for a different request, so it will not be committed"
            )));
        }

        // Consumed, so a plan is good for exactly one action.
        self.plans.remove(id);
        Ok(plan)
    }

    /// Drops plans past their time to live.
    fn expire(&mut self, now: u64) {
        let ttl = self.ttl_secs;
        self.plans
            .retain(|_, plan| now.saturating_sub(plan.created_at) < ttl);
    }

    pub fn pending(&self) -> usize {
        self.plans.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str, args: &[&str]) -> AgentSpec {
        AgentSpec {
            id: AgentId::new(id).unwrap(),
            program: "/usr/bin/rm".into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            ceiling: RiskTier::Destructive,
        }
    }

    #[test]
    fn a_plan_commits_once() {
        let mut ledger = PlanLedger::new(120);
        let s = spec("tidy", &["/tmp/scratch"]);

        let plan = ledger.propose(&s, RiskTier::Destructive, 100).unwrap();
        assert_eq!(ledger.pending(), 1);

        ledger.commit(&plan.id, &s, 101).unwrap();
        assert_eq!(ledger.pending(), 0);

        // A second commit finds nothing, so a plan cannot be replayed.
        assert!(ledger.commit(&plan.id, &s, 102).is_err());
    }

    /// The whole reason the ledger exists. Agreeing to one thing must not authorise another.
    #[test]
    fn a_plan_cannot_be_committed_against_a_different_spec() {
        let mut ledger = PlanLedger::new(120);
        let harmless = spec("tidy", &["/tmp/scratch"]);
        let dangerous = spec("tidy", &["/home/jj_tmc"]);

        let plan = ledger
            .propose(&harmless, RiskTier::Destructive, 100)
            .unwrap();
        let err = ledger.commit(&plan.id, &dangerous, 101).unwrap_err();

        assert!(err.to_string().contains("different request"), "{err}");
        // And the plan survives, so the mistake does not also destroy the honest one.
        assert!(ledger.commit(&plan.id, &harmless, 102).is_ok());
    }

    /// Even a one character difference must be caught.
    #[test]
    fn the_smallest_change_is_still_a_different_request() {
        let mut ledger = PlanLedger::new(120);
        let planned = spec("tidy", &["/tmp/scratch"]);
        let committed = spec("tidy", &["/tmp/scratch/"]);

        let plan = ledger
            .propose(&planned, RiskTier::Destructive, 100)
            .unwrap();
        assert!(ledger.commit(&plan.id, &committed, 101).is_err());
    }

    #[test]
    fn a_plan_expires() {
        let mut ledger = PlanLedger::new(120);
        let s = spec("tidy", &["/tmp/scratch"]);
        let plan = ledger.propose(&s, RiskTier::Destructive, 100).unwrap();

        assert!(ledger.commit(&plan.id, &s, 219).is_ok());

        let plan = ledger.propose(&s, RiskTier::Destructive, 300).unwrap();
        let err = ledger.commit(&plan.id, &s, 420).unwrap_err();
        assert!(err.to_string().contains("expired"), "{err}");
    }

    #[test]
    fn an_invented_plan_id_is_refused() {
        let mut ledger = PlanLedger::new(120);
        let s = spec("tidy", &["/tmp/scratch"]);
        let err = ledger
            .commit(&PlanId::from("deadbeef".to_string()), &s, 100)
            .unwrap_err();
        assert!(err.to_string().contains("no plan"), "{err}");
    }

    /// Ids must not be guessable from one another.
    #[test]
    fn plan_ids_are_random_and_long() {
        let ids: std::collections::BTreeSet<_> =
            (0..50).map(|_| PlanId::fresh().unwrap()).collect();
        assert_eq!(ids.len(), 50, "ids repeated");
        assert!(ids.iter().all(|id| id.as_str().len() == 32));
    }
}
