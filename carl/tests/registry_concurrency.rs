use carl::{Registry, ThreadId};
use std::sync::{Arc, Barrier};

fn thread(name: &str) -> ThreadId {
    ThreadId::new(name).unwrap()
}

#[test]
fn finishing_an_answer_preserves_a_thread_created_by_another_registry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("threads.json");
    let mut answering = Registry::open(&path).unwrap();
    answering.session_for(&thread("first"), 1).unwrap();
    let mut other = Registry::open(&path).unwrap();
    let (session, _) = other.session_for(&thread("second"), 2).unwrap();
    answering.record_turn(&thread("first")).unwrap();
    let saved = Registry::open(&path).unwrap();
    assert_eq!(
        saved.get(&thread("second")).map(|entry| &entry.session),
        Some(&session)
    );
    assert_eq!(saved.get(&thread("first")).unwrap().turns, 1);
}

#[test]
fn stale_registries_reuse_the_same_new_session_and_preserve_turn_counts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("threads.json");
    let mut first = Registry::open(&path).unwrap();
    let mut second = Registry::open(&path).unwrap();
    let (session, _) = first.session_for(&thread("shared"), 1).unwrap();
    let (reused, minted) = second.session_for(&thread("shared"), 2).unwrap();
    assert_eq!(reused, session);
    assert!(!minted);
    first.record_turn(&thread("shared")).unwrap();
    second.record_turn(&thread("shared")).unwrap();
    assert_eq!(
        Registry::open(&path)
            .unwrap()
            .get(&thread("shared"))
            .unwrap()
            .turns,
        2
    );
}

#[test]
fn concurrent_registry_writers_preserve_every_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("threads.json");
    let gate = Arc::new(Barrier::new(12));
    let workers: Vec<_> = (0..12)
        .map(|i| {
            let gate = gate.clone();
            let path = path.clone();
            std::thread::spawn(move || {
                let mut registry = Registry::open(path).unwrap();
                gate.wait();
                registry
                    .session_for(&thread(&format!("thread-{i}")), i)
                    .unwrap();
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(Registry::open(path).unwrap().len(), 12);
}
