//! Overlay observed repair work without inventing a conversational task assignment.
use crate::model::{AgentStatus, AgentView, Snapshot};
use carl::providers::army::workflow::COMPONENT;
use carl::providers::health::Reading;

pub(super) fn apply(snapshot: &mut Snapshot) -> Option<AgentView> {
    let diagnostic = snapshot
        .diagnostics
        .iter()
        .find(|d| d.component == COMPONENT)?;
    let phase = diagnostic.metrics.iter().find(|m| m.name == "phase")?;
    let Reading::Text(phase) = &phase.value else {
        return None;
    };
    let agent = snapshot.agents.iter_mut().find(|a| a.name == "evan")?;
    // A task explicitly held by Evan retains priority over background workflow observations.
    if agent.task.is_some() {
        return None;
    }
    agent.status = match phase.as_str() {
        "review" => AgentStatus::AwaitingReview,
        "working" => AgentStatus::Working,
        "blocked" => AgentStatus::Blocked,
        "idle" | "unconfigured" => AgentStatus::Idle,
        _ => AgentStatus::Unknown,
    };
    agent.blocker = (agent.status == AgentStatus::Blocked).then(|| diagnostic.summary.clone());
    agent.last_activity = Some(diagnostic.summary.clone());
    // The workflow timestamps are not the conversational journal's timestamps.
    agent.last_activity_at = None;
    Some(agent.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{MockPanelDataSource, PanelDataSource};
    use carl::providers::health::{Diagnostic, Health, Kind, Metric};

    #[test]
    fn ready_repairs_are_visible_without_a_fabricated_task() {
        let mut snapshot = MockPanelDataSource::new().snapshot();
        let evan = snapshot
            .agents
            .iter_mut()
            .find(|a| a.name == "evan")
            .unwrap();
        evan.task = None;
        snapshot.diagnostics.push(
            Diagnostic::new(
                COMPONENT,
                Health::Healthy,
                "Holoprojector #2 is ready",
                Kind::EventDriven,
            )
            .with(Metric::new("phase", Reading::Text("review".into()), "")),
        );
        let changed = apply(&mut snapshot).unwrap();
        assert_eq!(changed.status, AgentStatus::AwaitingReview);
        assert!(changed.task.is_none());
        assert!(changed.last_activity.unwrap().contains("Holoprojector #2"));
        snapshot
            .agents
            .iter_mut()
            .find(|a| a.name == "evan")
            .unwrap()
            .task = Some("real-task".into());
        assert!(apply(&mut snapshot).is_none());
    }
}
