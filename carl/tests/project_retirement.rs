//! Retiring Projects must preserve tasks and leave historical files untouched.
use carl::army::personnel::found;
use carl::panel::{client::PanelClient, snapshot};
use serde_json::json;

mod common;
use common::Backend;

#[test]
fn historical_project_fields_do_not_return_in_task_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let people = found(dir.path(), 1).unwrap();
    let old = json!({
        "seq": 1, "at": 100, "actor": "adrian", "event": "delegated",
        "task": "old-task", "to": "evan", "goal": "Finish the reviewed repair",
        "must": ["regression passes"], "project": "legacy-project",
        "workspace": "/tmp/allowed", "objective": 7
    });
    std::fs::create_dir_all(people.journal_path().parent().unwrap()).unwrap();
    std::fs::write(people.journal_path(), format!("{old}\n")).unwrap();
    let bytes = std::fs::read(people.journal_path()).unwrap();
    let snapshot = snapshot::build(dir.path()).unwrap();
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].owner, "evan");
    assert_eq!(snapshot.tasks[0].must, vec!["regression passes"]);
    let value = serde_json::to_value(snapshot).unwrap();
    assert!(
        value.get("projects").is_none(),
        "retired projects must not be on the wire"
    );
    assert!(value["tasks"][0].get("project").is_none());
    assert_eq!(std::fs::read(people.journal_path()).unwrap(), bytes);
}

#[test]
fn live_backend_ignores_legacy_project_files_without_changing_them() {
    let dir = tempfile::tempdir().unwrap();
    found(dir.path(), 1).unwrap();
    let legacy = dir.path().join("projects/old");
    std::fs::create_dir_all(&legacy).unwrap();
    let bytes = b"old project state, deliberately not valid JSON\n";
    std::fs::write(legacy.join("project.json"), bytes).unwrap();
    let backend = Backend::start(dir.path());
    let snapshot = PanelClient::connect(&backend.socket())
        .unwrap()
        .snapshot()
        .unwrap();
    let value = serde_json::to_value(snapshot).unwrap();
    assert!(value.get("projects").is_none());
    assert!(value["tasks"].is_array());
    assert_eq!(std::fs::read(legacy.join("project.json")).unwrap(), bytes);
}
