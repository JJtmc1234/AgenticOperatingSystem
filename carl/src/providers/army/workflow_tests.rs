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
        events
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let mut event = value.clone();
                if event.get("seq").is_none() {
                    event["seq"] = json!(index + 1);
                }
                format!("{event}\n")
            })
            .collect::<String>(),
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

#[test]
fn a_gap_in_evans_journal_cannot_claim_a_repair_is_ready() {
    let dir = home(&[json!({"seq":2,"kind":"run_finished","report":{"rows":[{
        "repo":"JJtmc1234/Holoprojector","issue":2,"status":"Prepared. Publication disabled."
    }]}})]);
    assert_eq!(reading(dir.path()).health, Health::Unknown);
}

#[test]
fn a_malformed_workflow_report_is_unknown_not_idle() {
    let dir = home(&[json!({"seq":1,"kind":"run_finished","report":{"rows":[{
        "repo":"JJtmc1234/Holoprojector","issue":2
    }]}})]);
    assert_eq!(reading(dir.path()).health, Health::Unknown);
}

#[test]
fn a_review_command_uses_only_valid_repository_and_issue_arguments() {
    for (repo, issue, expected) in [
        ("JJtmc1234/Holoprojector", 2, true),
        ("JJtmc1234/repo;touch /tmp/wrong", 2, false),
        ("--help/repo", 2, false),
        ("JJtmc1234/Holoprojector", 0, false),
    ] {
        let dir = home(&[json!({"kind":"run_finished","report":{"rows":[{
            "repo":repo,"issue":issue,"status":"Prepared. Publication disabled."
        }]}})]);
        let found = reading(dir.path());
        let command = found.metrics.iter().find(|m| m.name == "review_command");
        assert_eq!(command.is_some(), expected);
        if let Some(command) = command {
            assert_eq!(
                command.value,
                Reading::Text("carl evan review --repo JJtmc1234/Holoprojector --issue 2".into())
            );
        }
    }
}

#[test]
fn queued_or_retrying_repairs_do_not_look_like_an_empty_workflow() {
    for status in [
        "Queued. Evan daily model budget exhausted. Work remains queued.",
        "Waiting for two hour retry or changed issue or source.",
        "Blocked. Prepared checkout changed. Saved verification no longer describes it.",
    ] {
        let dir = home(&[json!({"kind":"run_finished","report":{"rows":[{
            "repo":"JJtmc1234/Holoprojector","issue":2,"status":status
        }]}})]);
        let found = reading(dir.path());
        assert_eq!(found.health, Health::Blocked);
        assert!(found.summary.contains(status));
    }
}
