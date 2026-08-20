//! What an agent is allowed to do, as data rather than as prose.
//!
//! The tier says how much damage something could do. The policy says what to do about that.
//! Keeping them apart means a tier never has to be edited to change a decision.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{AgentId, Error, Result, RiskTier};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Goes ahead on its own.
    Allow,
    /// Needs a plan and then an explicit commit.
    Prompt,
    /// Never, whatever the caller says.
    Deny,
}

/// Spelled the same way the policy file spells it, so what an operator is shown matches what
/// they would have to write to change it.
impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Verdict::Allow => "allow",
            Verdict::Prompt => "prompt",
            Verdict::Deny => "deny",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Verdict per tier. Every tier must be listed, so adding one is a deliberate decision
    /// rather than something that silently inherits a default.
    pub tiers: BTreeMap<RiskTier, Verdict>,

    /// Per agent overrides, which win over the tier.
    #[serde(default)]
    pub agents: BTreeMap<AgentId, Verdict>,

    /// How long a plan stays committable.
    #[serde(default = "default_plan_ttl")]
    pub plan_ttl_secs: u64,
}

const fn default_plan_ttl() -> u64 {
    120
}

impl Default for Policy {
    /// The safe default, used when no policy file exists.
    ///
    /// Read runs freely because it changes nothing. Everything else needs a human, because a
    /// runtime that starts out permissive is one that gets deployed permissive.
    fn default() -> Self {
        Self {
            tiers: BTreeMap::from([
                (RiskTier::Read, Verdict::Allow),
                (RiskTier::Write, Verdict::Prompt),
                (RiskTier::System, Verdict::Prompt),
                (RiskTier::Destructive, Verdict::Prompt),
            ]),
            agents: BTreeMap::new(),
            plan_ttl_secs: default_plan_ttl(),
        }
    }
}

impl Policy {
    /// Loads a policy, or returns the safe default when the file is absent.
    ///
    /// Only for a path nobody named, where a missing file genuinely means "no policy written,
    /// use the default". A run directory with no `policy.toml` is that case and it is
    /// documented behaviour. A path somebody typed is not: use `load_required`, or a typo
    /// silently swaps their rules for different ones. See bug 7.
    ///
    /// A malformed policy is an error rather than a fallback to the default. Quietly running
    /// under different rules than the ones written down is the worst outcome available.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        Self::parse(&text, path)
    }

    /// Loads a policy that has to be there, refusing to fall back to the default.
    ///
    /// For a path an operator supplied. Naming a file is a statement that its rules are the
    /// ones to enforce, so a missing file is a mistake to report rather than a preference to
    /// infer. The default is more permissive than most policies anybody bothers to write, so
    /// falling back to it turns a typo into a quiet loosening.
    pub fn load_required(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| {
            Error::Refused(format!(
                "the policy at {} could not be read, and a named policy is not optional \
                 because falling back to the built in default would enforce different rules \
                 than the ones asked for: {e}",
                path.display()
            ))
        })?;
        Self::parse(&text, path)
    }

    fn parse(text: &str, path: &Path) -> Result<Self> {
        let policy: Self = toml::from_str(text).map_err(|e| {
            Error::Refused(format!("{} is not a valid policy: {e}", path.display()))
        })?;
        policy.check()?;
        Ok(policy)
    }

    /// The verdicts, in tier order, for showing an operator what is actually in force.
    pub fn summary(&self) -> String {
        self.tiers
            .iter()
            .map(|(tier, verdict)| format!("{tier}={verdict}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Refuses a policy that does not cover every tier.
    ///
    /// A missing tier would have to mean something, and every available meaning is bad. Deny
    /// silently breaks working setups, allow silently opens a hole. So it is an error.
    fn check(&self) -> Result<()> {
        let missing: Vec<_> = RiskTier::ALL
            .iter()
            .filter(|tier| !self.tiers.contains_key(tier))
            .map(|tier| tier.to_string())
            .collect();

        if missing.is_empty() {
            Ok(())
        } else {
            Err(Error::Refused(format!(
                "policy does not say what to do about tier(s): {}",
                missing.join(", ")
            )))
        }
    }

    /// The verdict for one agent at one tier. An agent override wins over the tier.
    pub fn verdict(&self, agent: &AgentId, tier: RiskTier) -> Verdict {
        if let Some(v) = self.agents.get(agent) {
            return *v;
        }
        // `check` guarantees the tier is present, and `Deny` is the safe answer if that ever
        // stops being true.
        self.tiers.get(&tier).copied().unwrap_or(Verdict::Deny)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> AgentId {
        AgentId::new(name).unwrap()
    }

    /// The bug. `--policy` is a required argument on `aos-files`, so a path that is not there
    /// is a mistake to report rather than a preference to infer. `load` inferred it, and the
    /// built in default is more permissive than most policies anybody writes, so a typo turned
    /// `destructive = "deny"` into `prompt` with nothing said about it.
    #[test]
    fn a_named_policy_that_is_missing_is_an_error_not_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.toml");

        // The behaviour that caused it, kept deliberately for the unnamed case.
        assert_eq!(Policy::load(&missing).unwrap(), Policy::default());

        let e = Policy::load_required(&missing).unwrap_err().to_string();
        assert!(e.contains("not optional"), "{e}");
    }

    /// A run directory with no policy file is the case `load` exists for, and it is documented
    /// behaviour rather than an accident. Pinned so the fix above does not get generalised
    /// into it later.
    #[test]
    fn an_unnamed_missing_policy_is_still_the_safe_default() {
        let dir = tempfile::tempdir().unwrap();
        let policy = Policy::load(dir.path().join("policy.toml")).unwrap();
        assert_eq!(
            policy.verdict(&id("anything"), RiskTier::Read),
            Verdict::Allow
        );
        assert_eq!(
            policy.verdict(&id("anything"), RiskTier::Destructive),
            Verdict::Prompt
        );
    }

    /// Both loaders have to refuse a file that is there and malformed, which is the case that
    /// was already right and must stay right.
    #[test]
    fn a_malformed_named_policy_is_still_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy.toml");
        std::fs::write(&path, "this is not toml at all [[[").unwrap();

        assert!(Policy::load(&path).is_err());
        assert!(Policy::load_required(&path).is_err());
    }

    /// What an operator is shown has to be spelled the way they would have to write it, or the
    /// banner is a second vocabulary to learn.
    #[test]
    fn the_summary_uses_the_words_the_policy_file_uses() {
        let s = Policy::default().summary();
        assert!(s.contains("read=allow"), "{s}");
        assert!(s.contains("destructive=prompt"), "{s}");
    }

    #[test]
    fn the_default_allows_only_read() {
        let policy = Policy::default();
        assert_eq!(policy.verdict(&id("any"), RiskTier::Read), Verdict::Allow);
        for tier in [RiskTier::Write, RiskTier::System, RiskTier::Destructive] {
            assert_eq!(policy.verdict(&id("any"), tier), Verdict::Prompt);
        }
    }

    #[test]
    fn a_missing_file_gives_the_default() {
        let policy = Policy::load("/tmp/aos-no-such-policy.toml").unwrap();
        assert_eq!(policy, Policy::default());
    }

    #[test]
    fn an_agent_override_beats_the_tier() {
        let policy: Policy = toml::from_str(
            r#"
            [tiers]
            read = "allow"
            write = "prompt"
            system = "prompt"
            destructive = "deny"

            [agents]
            "morning-brief" = "allow"
            "#,
        )
        .unwrap();

        assert_eq!(
            policy.verdict(&id("morning-brief"), RiskTier::Destructive),
            Verdict::Allow
        );
        assert_eq!(
            policy.verdict(&id("anyone-else"), RiskTier::Destructive),
            Verdict::Deny
        );
    }

    /// A policy that does not cover a tier must be refused rather than guessed at.
    #[test]
    fn a_policy_missing_a_tier_is_refused() {
        let err = toml::from_str::<Policy>(
            r#"
            [tiers]
            read = "allow"
            write = "prompt"
            "#,
        )
        .map_err(|e| e.to_string())
        .and_then(|p: Policy| p.check().map_err(|e| e.to_string()))
        .unwrap_err();

        assert!(err.contains("system"), "{err}");
        assert!(err.contains("destructive"), "{err}");
    }

    /// A broken policy file must not silently fall back to the default.
    #[test]
    fn a_malformed_policy_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy.toml");
        std::fs::write(&path, "this is not toml {{{").unwrap();

        assert!(Policy::load(&path).is_err());
    }

    #[test]
    fn an_unknown_verdict_is_refused() {
        assert!(
            toml::from_str::<Policy>(
                r#"
                [tiers]
                read = "maybe"
                write = "prompt"
                system = "prompt"
                destructive = "deny"
                "#,
            )
            .is_err()
        );
    }
}
