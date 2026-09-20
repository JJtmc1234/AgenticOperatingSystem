//! A launched process is not yet a resumable conversation.
use super::*;
use crate::army::personnel::{Personnel, found};
use std::{path::Path, time::Duration};

fn program(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/stand-in")
        .join(name)
}

#[test]
fn an_idle_process_does_not_establish_a_conversation_for_restart() {
    let home = tempfile::tempdir().unwrap();
    found(home.path(), 100).unwrap();
    let people = Personnel::open(home.path()).unwrap();
    let id = people.identity("evan").unwrap().id.clone();
    let mut supervisor = Supervisor::take(home.path(), program("agent-stays-up")).unwrap();
    supervisor.tick(&people, 1000).unwrap();
    assert!(!supervisor.roll().get(&id).unwrap().established);
    let old = supervisor.roll().get(&id).unwrap().session.clone().unwrap();
    drop(supervisor);
    let mut supervisor = Supervisor::take(home.path(), program("agent-stays-up")).unwrap();
    let tick = supervisor.tick(&people, 2000).unwrap();
    assert!(
        tick.what
            .iter()
            .any(|(name, result)| name == "evan" && *result == Outcome::Started(Start::Fresh))
    );
    assert!(supervisor.roll().get(&id).unwrap().abandoned.contains(&old));
}

#[test]
fn an_answer_establishes_the_conversation_before_it_can_be_resumed() {
    let home = tempfile::tempdir().unwrap();
    found(home.path(), 100).unwrap();
    let people = Personnel::open(home.path()).unwrap();
    let id = people.identity("evan").unwrap().id.clone();
    let mut supervisor = Supervisor::take(home.path(), program("agent-does-as-told")).unwrap();
    supervisor.tick(&people, 1000).unwrap();
    assert!(!supervisor.roll().get(&id).unwrap().established);
    supervisor
        .deliver(&id, "hello", Duration::from_secs(2))
        .unwrap();
    let session = supervisor.roll().get(&id).unwrap().session.clone();
    assert!(supervisor.roll().get(&id).unwrap().established);
    drop(supervisor);
    let mut supervisor = Supervisor::take(home.path(), program("agent-does-as-told")).unwrap();
    let tick = supervisor.tick(&people, 2000).unwrap();
    assert!(
        tick.what
            .iter()
            .any(|(name, result)| name == "evan" && *result == Outcome::Started(Start::Resume))
    );
    assert_eq!(supervisor.roll().get(&id).unwrap().session, session);
}

#[test]
fn an_interrupted_first_delivery_does_not_claim_a_completed_conversation() {
    let home = tempfile::tempdir().unwrap();
    found(home.path(), 100).unwrap();
    let people = Personnel::open(home.path()).unwrap();
    let id = people.identity("evan").unwrap().id.clone();
    let mut supervisor = Supervisor::take(home.path(), program("agent-stays-up")).unwrap();
    supervisor.tick(&people, 1000).unwrap();
    assert!(
        supervisor
            .deliver(&id, "hello", Duration::from_millis(1))
            .is_err()
    );
    assert!(!supervisor.roll().get(&id).unwrap().established);
    let records = crate::army::event::read(home.path().join("run/events.jsonl")).unwrap();
    assert!(!records.iter().any(|r| matches!(
        r.event,
        crate::army::event::Event::AgentSessionEstablished { .. }
    )));
}

#[test]
fn an_establishment_record_survives_a_crash_before_the_runtime_cache_is_saved() {
    let home = tempfile::tempdir().unwrap();
    found(home.path(), 100).unwrap();
    let people = Personnel::open(home.path()).unwrap();
    let id = people.identity("evan").unwrap().id.clone();
    let mut supervisor = Supervisor::take(home.path(), program("agent-does-as-told")).unwrap();
    supervisor.tick(&people, 1000).unwrap();
    let before = supervisor.roll().get(&id).unwrap().clone();
    supervisor
        .deliver(&id, "hello", Duration::from_secs(2))
        .unwrap();
    let mut stale = Roll::open(home.path()).unwrap();
    stale.save(home.path(), before).unwrap();
    drop(supervisor);
    let supervisor = Supervisor::take(home.path(), program("agent-does-as-told")).unwrap();
    assert!(supervisor.roll().get(&id).unwrap().established);
}
