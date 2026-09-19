use super::*;
use serde_json::json;

fn home(events: &[serde_json::Value]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("evan");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("config.json"),
        r#"{"repositories":{"JJtmc1234/Holoprojector":{}}}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("events.jsonl"),
        events.iter().map(|v| format!("{v}\n")).collect::<String>(),
    )
    .unwrap();
    dir
}

fn reading(dir: &std::path::Path) -> Diagnostic {
    Army::new(dir)
        .records()
        .into_iter()
        .find(|d| d.component == "army.workflow.evan")
        .expect("Evan's repair workflow must reach the panel")
}

#[test]
fn prepared_evan_repairs_reach_the_panel_as_review_work() {
    let dir = home(&[json!({"kind":"run_finished","report":{"rows":[{
        "repo":"JJtmc1234/Holoprojector","issue":2,"status":"Prepared. Publication disabled."
    }]}})]);
    let d = reading(dir.path());
    assert_eq!(d.health, Health::Healthy);
    assert!(d.summary.contains("Holoprojector #2"));
    assert!(
        d.metrics
            .iter()
            .any(|m| m.name == "phase" && m.value == Reading::Text("review".into()))
    );
}

#[test]
fn interrupted_evan_run_is_not_reported_as_working_forever() {
    let dir = home(&[json!({"kind":"run_started"})]);
    let d = reading(dir.path());
    assert_eq!(d.health, Health::Blocked);
    assert!(d.summary.contains("interrupted"));
}

#[test]
fn malformed_evan_journal_is_unknown_not_idle() {
    let dir = home(&[]);
    std::fs::write(dir.path().join("evan/events.jsonl"), "broken\n").unwrap();
    assert_eq!(reading(dir.path()).health, Health::Unknown);
}

#[test]
fn a_held_workflow_lock_means_working_and_an_unheld_file_does_not() {
    use std::os::fd::AsRawFd;
    let dir = home(&[json!({"kind":"run_started"})]);
    let file = std::fs::File::create(dir.path().join("evan/run.lock")).unwrap();
    // This test owns the descriptor for the whole exclusive lock lifetime.
    assert_eq!(
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let d = reading(dir.path());
    assert!(
        d.metrics
            .iter()
            .any(|m| m.value == Reading::Text("working".into()))
    );
    drop(file);
    assert_eq!(reading(dir.path()).health, Health::Blocked);
}
