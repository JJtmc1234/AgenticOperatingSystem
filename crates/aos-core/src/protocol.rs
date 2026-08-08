//! What the cli and the daemon say to each other.
//!
//! One JSON object per line, in both directions. Readable with `nc` when the daemon is the
//! thing that is broken, which is the same reason the event log is text.
//!
//! Lives in `aos-core` so both sides compile against one definition. A protocol defined twice
//! is a protocol that drifts.

use serde::{Deserialize, Serialize};

use crate::{AgentId, AgentSpec, AgentState, PlanId, ProcessHandle, RiskTier};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case")]
pub enum Request {
    /// Is anyone there, and what do they think they are holding.
    Ping,
    List,
    Start {
        spec: Box<AgentSpec>,
        /// Quote a plan's id to commit it. Absent means "tell me what would happen".
        ///
        /// Optional so the planning call and the committing call are the same request with
        /// one field added. Two different verbs would let a caller reach the acting one
        /// without ever passing through the planning one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<PlanId>,
    },
    Stop {
        agent: AgentId,
        #[serde(default = "default_grace")]
        grace_secs: u64,
    },
    /// Kill switch. Every agent, whatever its tier.
    StopAll {
        #[serde(default = "default_grace")]
        grace_secs: u64,
    },
}

const fn default_grace() -> u64 {
    5
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentReport {
    pub id: AgentId,
    pub state: AgentState,
    /// True when this agent outlived a previous daemon, so its exit code is unknowable.
    pub adopted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "response", rename_all = "snake_case")]
pub enum Response {
    Pong {
        version: String,
        tracking: usize,
    },
    Agents {
        agents: Vec<AgentReport>,
    },
    Started {
        handle: ProcessHandle,
    },
    /// The action needs a commit. Nothing has happened yet.
    PlanRequired {
        plan: PlanId,
        agent: AgentId,
        tier: RiskTier,
        /// What committing would actually do, in one line a human can check.
        summary: String,
    },
    Stopped {
        agent: AgentId,
        state: AgentState,
    },
    StoppedAll {
        stopped: Vec<AgentId>,
        failed: Vec<String>,
    },
    /// Every refusal comes back this way, never as a dropped connection.
    Error {
        message: String,
    },
}

impl Response {
    pub fn error(message: impl std::fmt::Display) -> Self {
        Response::Error {
            message: message.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RiskTier;

    fn round_trip<T>(value: &T) -> T
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap()
    }

    #[test]
    fn requests_round_trip() {
        let spec = AgentSpec {
            id: AgentId::new("brief").unwrap(),
            program: "/usr/bin/echo".into(),
            args: vec!["hi".into()],
            ceiling: RiskTier::Read,
        };
        for request in [
            Request::Ping,
            Request::List,
            Request::Start {
                spec: Box::new(spec.clone()),
                commit: None,
            },
            Request::Start {
                spec: Box::new(spec),
                commit: Some(crate::PlanId::from("abc123".to_string())),
            },
            Request::Stop {
                agent: AgentId::new("brief").unwrap(),
                grace_secs: 3,
            },
            Request::StopAll { grace_secs: 5 },
        ] {
            assert_eq!(round_trip(&request), request);
        }
    }

    #[test]
    fn responses_round_trip() {
        let response = Response::Agents {
            agents: vec![AgentReport {
                id: AgentId::new("brief").unwrap(),
                state: AgentState::Running { pid: 42 },
                adopted: true,
            }],
        };
        assert_eq!(round_trip(&response), response);
    }

    /// Omitting the grace period must not fail the whole request. A caller that forgets it
    /// should get the default, not a parse error it cannot act on.
    #[test]
    fn grace_defaults_when_left_out() {
        let parsed: Request =
            serde_json::from_str(r#"{"request":"stop","agent":"brief"}"#).unwrap();
        assert_eq!(
            parsed,
            Request::Stop {
                agent: AgentId::new("brief").unwrap(),
                grace_secs: 5
            }
        );
    }

    /// A start with no commit field must parse, because that is the planning call.
    #[test]
    fn a_start_without_a_commit_is_a_plan_request() {
        let parsed: Request = serde_json::from_str(
            r#"{"request":"start","spec":{"id":"x","program":"/usr/bin/echo","ceiling":"read"}}"#,
        )
        .unwrap();
        assert!(matches!(parsed, Request::Start { commit: None, .. }));
    }

    /// The schema is checked before anything is acted on. A bad agent id in a start request
    /// is refused at parse time, so it never reaches the supervisor.
    #[test]
    fn a_path_shaped_agent_id_is_refused_at_parse_time() {
        let bad = r#"{"request":"stop","agent":"../../etc/passwd"}"#;
        assert!(serde_json::from_str::<Request>(bad).is_err());
    }

    #[test]
    fn an_unknown_request_is_refused_rather_than_ignored() {
        assert!(serde_json::from_str::<Request>(r#"{"request":"rm_rf"}"#).is_err());
    }
}
