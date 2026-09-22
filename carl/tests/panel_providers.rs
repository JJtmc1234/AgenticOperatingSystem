//! The providers as the panel sees them, over the real socket.
//!
//! The questions here are all about what survives the trip. The collectors are careful to keep
//! two distinctions that a careless wire format would flatten: a reading that could not be taken
//! is not zero, and a fact that is true until something changes it is not a fact measured at an
//! instant. Both of those are easy to lose in a `f64` and a `u64`, and neither would look wrong
//! on screen once lost, which is why they are checked here rather than trusted.

use std::path::Path;

use carl::army::event::{Event, Journal};
use carl::army::personnel::found;
use carl::army::task::{Status, Task};
use carl::panel::client::PanelClient;
use carl::providers::health::{Health, Kind, Metric, Reading};

mod common;
use common::{Backend, verification};

/// A home with an army and one recorded task.
fn a_working_home(dir: &Path) -> Task {
    let people = found(dir, 1).unwrap();

    let t = Task::assign(
        "mason",
        "nora",
        "cache the prototype lookup",
        verification(),
    )
    .unwrap();

    let mut journal = Journal::open(people.journal_path()).unwrap();
    journal
        .append(
            "mason",
            Event::Delegated {
                task: t.id.clone(),
                to: "nora".into(),
                goal: t.goal.clone(),
                parent: None,
                must: t.verification.must.clone(),

                workspace: None,
                objective: None,
            },
        )
        .unwrap();
    journal
        .append(
            "nora",
            Event::moved(&t.id, Status::Assigned, Status::InHand),
        )
        .unwrap();
    t
}

#[test]
fn a_task_shows_its_recorded_owner_without_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let t = a_working_home(dir.path());
    let backend = Backend::start(dir.path());
    let snapshot = PanelClient::connect(&backend.socket())
        .unwrap()
        .snapshot()
        .unwrap();
    let task = snapshot
        .tasks
        .iter()
        .find(|x| x.id == t.id.to_string())
        .unwrap();
    assert_eq!(task.owner, "nora");
    assert_eq!(task.assigner, "mason");
    assert_eq!(task.status, "in hand");
    assert_eq!(task.goal, t.goal);
}

/// Tasks need no project store.
#[test]
fn an_independent_task_survives_the_wire() {
    let dir = tempfile::tempdir().unwrap();
    let t = a_working_home(dir.path());
    let backend = Backend::start(dir.path());
    let snapshot = PanelClient::connect(&backend.socket())
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].id, t.id.to_string());
    assert!(!dir.path().join("projects").exists());
}

/// The distinction the collectors are careful about, checked after a JSON round trip.
#[test]
fn an_unknown_reading_is_still_unknown_after_crossing_the_socket() {
    let unknown = Metric::unknown("gpu.temperature", "C");
    let known = Metric::new("cpu.load", Reading::Float(0.5), "%");

    for m in [&unknown, &known] {
        let text = serde_json::to_string(m).unwrap();
        let back: Metric = serde_json::from_str(&text).unwrap();
        assert_eq!(&back, m, "{text}");
    }

    // The part that matters. Unknown must not arrive as a number anybody could act on.
    assert_eq!(unknown.value, Reading::Unknown);
    assert_eq!(unknown.value.as_f64(), None, "and never reads as zero");
    assert_ne!(unknown.value, Reading::Float(0.0));
    assert!(!unknown.value.is_known());
    assert_eq!(unknown.unit, "C", "what it would have been is still useful");
}

/// A sampled reading nobody could take still says when the attempt was made.
#[test]
fn a_sampled_unknown_keeps_its_measured_at() {
    let d = carl::providers::health::Diagnostic::new(
        "system.gpu",
        Health::Unknown,
        "no supported GPU found",
        Kind::Sampled,
    )
    .with(Metric::unknown("gpu.temperature", "C"))
    .measured(1_755_200_000);

    let back: carl::providers::health::Diagnostic =
        serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();

    assert_eq!(back.measured_at, Some(1_755_200_000), "when we looked");
    assert_eq!(back.health, Health::Unknown, "and that we found nothing");
    assert_eq!(back.metrics[0].value, Reading::Unknown);
    assert_eq!(back.kind, Kind::Sampled);
}

#[test]
fn the_two_kinds_of_diagnostic_stay_apart_on_the_wire() {
    let dir = tempfile::tempdir().unwrap();
    found(dir.path(), 1).unwrap();
    let backend = Backend::start(dir.path());

    let snapshot = PanelClient::connect(&backend.socket())
        .unwrap()
        .snapshot()
        .unwrap();

    let sampled: Vec<_> = snapshot
        .diagnostics
        .iter()
        .filter(|d| d.kind == Kind::Sampled)
        .collect();
    let event_driven: Vec<_> = snapshot
        .diagnostics
        .iter()
        .filter(|d| d.kind == Kind::EventDriven)
        .collect();

    assert!(!sampled.is_empty(), "the machine was read");
    assert!(!event_driven.is_empty(), "and the army was folded");

    for d in &sampled {
        assert!(
            d.measured_at.is_some(),
            "a sample without a moment is a sample you cannot age: {}",
            d.component
        );
    }
    for d in &event_driven {
        assert!(
            d.measured_at.is_none(),
            "army state is true until something changes it, not true at an instant: {}",
            d.component
        );
    }
}

/// Nothing associates a pid with an agent, so nothing may claim one is running.
#[test]
fn no_agent_claims_a_process_because_a_claude_is_running_somewhere() {
    let dir = tempfile::tempdir().unwrap();
    found(dir.path(), 1).unwrap();
    let backend = Backend::start(dir.path());

    let snapshot = PanelClient::connect(&backend.socket())
        .unwrap()
        .snapshot()
        .unwrap();

    for agent in &snapshot.agents {
        assert!(
            agent.process.is_unknown(),
            "{} claims a process state nothing can establish",
            agent.name
        );
    }
}

/// A restart must rebuild the link from the record, because that is where it lives.
#[test]
fn the_task_owner_and_requirements_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    a_working_home(dir.path());
    let mut backend = Backend::start(dir.path());
    let before = PanelClient::connect(&backend.socket())
        .unwrap()
        .snapshot()
        .unwrap();
    backend.down();
    backend.up();
    let after = PanelClient::connect(&backend.socket())
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(after.tasks, before.tasks);
    assert_eq!(after.tasks[0].owner, "nora");
    assert!(!after.tasks[0].must.is_empty());
}

/// A resync is replacement truth, not something to merge into what was already held.
#[test]
fn a_resynced_snapshot_replaces_provider_state_rather_than_merging_it() {
    use carl::panel::live::{LivePanel, Update};

    let dir = tempfile::tempdir().unwrap();
    a_working_home(dir.path());
    let backend = Backend::start(dir.path());

    let (mut live, first) = LivePanel::open(&backend.socket()).unwrap();
    assert_eq!(first.tasks.len(), 1);

    // Observe a new event before replacing the journal so this tests an established stream.
    let people = carl::army::personnel::Personnel::open(dir.path()).unwrap();
    let mut barrier = Journal::open(people.journal_path()).unwrap();
    barrier
        .append(
            "mason",
            Event::Decided {
                task: None,
                what: "stream ready".into(),
            },
        )
        .unwrap();
    let ready_by = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        assert!(
            std::time::Instant::now() < ready_by,
            "the stream never reached its barrier"
        );
        if matches!(live.next_update(), Update::Event(_)) {
            break;
        }
    }
    drop(barrier);

    // The record is replaced under the running panel, so the sequence it holds cannot be
    // honoured. Everything it was showing is now history.
    let people = carl::army::personnel::Personnel::open(dir.path()).unwrap();
    std::fs::write(people.journal_path(), "").unwrap();
    let mut journal = Journal::open(people.journal_path()).unwrap();
    journal
        .append(
            "mason",
            Event::Decided {
                task: None,
                what: "starting again".into(),
            },
        )
        .unwrap();

    let resync_by = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let fresh = loop {
        assert!(
            std::time::Instant::now() < resync_by,
            "the live stream never detected journal replacement"
        );
        match live.next_update() {
            Update::Resynced(s) => break s,
            Update::Health(_)
            | Update::Telemetry { .. }
            | Update::Asked(_)
            | Update::Answered { .. } => continue,
            Update::Event(e) => panic!("nothing was resumable: {}", e.seq),
        }
    };

    // The replacement is built from the record as it now is. The task that was linked is gone
    // from it, and no stale assignment is carried forward.
    assert!(
        fresh.tasks.is_empty(),
        "the record no longer holds that task"
    );
    assert_eq!(live.last_seq(), fresh.seq);
}
