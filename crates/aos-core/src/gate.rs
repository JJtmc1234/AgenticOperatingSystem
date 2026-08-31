//! The one place a request is judged before anything is started.
//!
//! This lived inside the daemon, so the daemon was the only path that checked policy. `aos run`
//! starts an agent in the foreground without going near `aosd`, and it went straight to the
//! supervisor, which meant a policy denying every tier was simply ignored by it. See bug 8.
//!
//! Deciding lives here. Recording the decision does not, because what a refusal looks like
//! differs between a socket reply and a message on a terminal, and the log those get written to
//! is owned by the caller. The gate answers what should happen. The caller carries it out and
//! writes it down.

use crate::{AgentSpec, PlanId, PlanLedger, Policy, RiskTier, Verdict};

/// What the caller should do about a request.
///
/// Every variant other than `Allow` means nothing has run and nothing will unless the caller
/// ignores the answer, which is why `Gate` hands back a value rather than a bool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Policy is satisfied. Go ahead.
    Allow,

    /// Refused outright, and the reason belongs in the log.
    Denied { reason: String },

    /// Above tier read with no commit quoted, so this was the planning call and nothing ran.
    Planned {
        plan: PlanId,
        tier: RiskTier,
        summary: String,
    },

    /// A plan was wanted and could not be made. Nothing ran and there is nothing to record
    /// against the agent, because it was never judged.
    CouldNotPlan { reason: String },

    /// A commit was quoted and it did not hold up against this exact request.
    CommitRefused { reason: String },
}

/// Policy plus the plans it has outstanding.
pub struct Gate {
    policy: Policy,
    /// Plans live in memory only. A plan is an offer, not a fact about the machine, and an offer
    /// that survived a restart would let someone commit something this process never proposed.
    plans: PlanLedger,
}

impl Gate {
    pub fn new(policy: Policy) -> Self {
        let plans = PlanLedger::new(policy.plan_ttl_secs);
        Self { policy, plans }
    }

    /// Loads the policy from a run directory. A missing file means the default, which allows
    /// only read.
    pub fn open(run_dir: &std::path::Path) -> crate::Result<Self> {
        Ok(Self::new(Policy::load(run_dir.join("policy.toml"))?))
    }

    pub fn plans_pending(&self) -> usize {
        self.plans.pending()
    }

    /// The full decision, for a caller that can hold a plan between two calls.
    pub fn decide(&mut self, spec: &AgentSpec, commit: Option<PlanId>, now: u64) -> Decision {
        let tier = spec.ceiling;

        match self.policy.verdict(&spec.id, tier) {
            Verdict::Allow => Decision::Allow,

            Verdict::Deny => Decision::Denied {
                reason: format!("policy denies {} at tier {tier}", spec.id),
            },

            Verdict::Prompt => match commit {
                None => match self.plans.propose(spec, tier, now) {
                    Ok(plan) => Decision::Planned {
                        plan: plan.id,
                        tier,
                        summary: format!(
                            "{} would run {} {:?} at tier {tier}",
                            spec.id, spec.program, spec.args
                        ),
                    },
                    Err(e) => Decision::CouldNotPlan {
                        reason: e.to_string(),
                    },
                },

                Some(id) => match self.plans.commit(&id, spec, now) {
                    Ok(_) => Decision::Allow,
                    Err(e) => Decision::CommitRefused {
                        reason: e.to_string(),
                    },
                },
            },
        }
    }

    /// The decision for a caller with nowhere to keep a plan between two calls.
    ///
    /// `aos run` is a single process that starts an agent and waits for it to finish. A plan
    /// offered by it would die with it, so there is no second call that could quote one and
    /// offering it would be a lie. Anything above allow is refused here and pointed at the
    /// daemon, which can hold the offer because it outlives the request.
    ///
    /// Takes `&self`, which is the useful part: with no plan to propose there is nothing to
    /// mutate, so this cannot leave state behind on a path that refused.
    pub fn decide_without_handshake(&self, spec: &AgentSpec) -> Decision {
        let tier = spec.ceiling;

        match self.policy.verdict(&spec.id, tier) {
            Verdict::Allow => Decision::Allow,

            Verdict::Deny => Decision::Denied {
                reason: format!("policy denies {} at tier {tier}", spec.id),
            },

            Verdict::Prompt => Decision::Denied {
                reason: format!(
                    "tier {tier} needs a commit and this command cannot hold a plan, so nothing \
                     has run. Start it through the daemon instead: aos start <spec>"
                ),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentId;
    use std::collections::BTreeMap;

    fn spec(id: &str, ceiling: RiskTier) -> AgentSpec {
        AgentSpec {
            id: AgentId::new(id).unwrap(),
            program: "/usr/bin/sleep".into(),
            args: vec!["1".into()],
            ceiling,
        }
    }

    fn denying_everything() -> Policy {
        Policy {
            tiers: BTreeMap::from([
                (RiskTier::Read, Verdict::Deny),
                (RiskTier::Write, Verdict::Deny),
                (RiskTier::System, Verdict::Deny),
                (RiskTier::Destructive, Verdict::Deny),
            ]),
            agents: BTreeMap::new(),
            plan_ttl_secs: 120,
        }
    }

    #[test]
    fn a_denying_policy_refuses_at_every_tier() {
        let mut gate = Gate::new(denying_everything());
        for tier in [
            RiskTier::Read,
            RiskTier::Write,
            RiskTier::System,
            RiskTier::Destructive,
        ] {
            assert!(matches!(
                gate.decide(&spec("hello", tier), None, 0),
                Decision::Denied { .. }
            ));
            assert!(matches!(
                gate.decide_without_handshake(&spec("hello", tier)),
                Decision::Denied { .. }
            ));
        }
    }

    /// The two entry points must not disagree about read, or which command you happened to use
    /// would decide whether the policy applied, which is exactly the bug this replaced.
    #[test]
    fn read_is_allowed_by_both_entry_points_under_the_default() {
        let mut gate = Gate::new(Policy::default());
        assert_eq!(
            gate.decide(&spec("hello", RiskTier::Read), None, 0),
            Decision::Allow
        );
        assert_eq!(
            gate.decide_without_handshake(&spec("hello", RiskTier::Read)),
            Decision::Allow
        );
    }

    /// A caller that cannot hold a plan is refused rather than handed one it can never redeem.
    #[test]
    fn a_prompt_tier_is_refused_without_a_handshake_rather_than_planned() {
        let gate = Gate::new(Policy::default());

        let decision = gate.decide_without_handshake(&spec("risky", RiskTier::Destructive));
        let Decision::Denied { reason } = decision else {
            panic!("a plan nobody can commit is not an offer, got {decision:?}");
        };
        assert!(reason.contains("aos start"), "{reason}");

        // And nothing was left behind, because the refusing path never proposes.
        assert_eq!(gate.plans_pending(), 0);
    }

    #[test]
    fn a_plan_can_be_proposed_then_committed_once() {
        let mut gate = Gate::new(Policy::default());
        let spec = spec("risky", RiskTier::Destructive);

        let Decision::Planned { plan, .. } = gate.decide(&spec, None, 0) else {
            panic!("prompt with no commit should plan");
        };
        assert_eq!(gate.decide(&spec, Some(plan.clone()), 1), Decision::Allow);
        assert!(matches!(
            gate.decide(&spec, Some(plan), 2),
            Decision::CommitRefused { .. }
        ));
    }

    /// An agent override beats the tier, and it has to beat it on both paths.
    #[test]
    fn a_per_agent_deny_beats_an_allowed_tier_on_both_paths() {
        let mut policy = Policy::default();
        policy
            .agents
            .insert(AgentId::new("wiper").unwrap(), Verdict::Deny);
        let mut gate = Gate::new(policy);

        assert!(matches!(
            gate.decide(&spec("wiper", RiskTier::Read), None, 0),
            Decision::Denied { .. }
        ));
        assert!(matches!(
            gate.decide_without_handshake(&spec("wiper", RiskTier::Read)),
            Decision::Denied { .. }
        ));
    }
}
