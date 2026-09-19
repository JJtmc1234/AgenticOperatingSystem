//! A separate repair journal must update an already connected panel.
use carl::panel::live::{LivePanel, Update};
use carl::providers::army::workflow::COMPONENT;
use carl::providers::health::Reading;
use std::time::{Duration, Instant};
mod common;
use common::Backend;

#[test]
fn workflow_completion_reaches_a_live_panel_without_changing_army_sequence() {
    let dir = tempfile::tempdir().unwrap();
    carl::army::personnel::found(dir.path(), 1).unwrap();
    let root = dir.path().join("evan");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("config.json"),
        r#"{"repositories":{"JJtmc1234/Holoprojector":{}}}"#,
    )
    .unwrap();
    let backend = Backend::start(dir.path());
    let (mut live, first) = LivePanel::open(&backend.socket()).unwrap();
    let sequence = first.seq;
    assert!(first.diagnostics.iter().any(|d| d.component == COMPONENT));
    let event = serde_json::json!({"kind":"run_finished","report":{"rows":[{
        "repo":"JJtmc1234/Holoprojector","issue":2,"status":"Prepared. Publication disabled."
    }]}});
    std::fs::write(root.join("events.jsonl"), format!("{event}\n")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            Instant::now() < deadline,
            "workflow completion never reached the panel"
        );
        if let Update::Telemetry { diagnostics, .. } = live.next_update()
            && diagnostics.iter().any(|d| {
                d.component == COMPONENT
                    && d.metrics
                        .iter()
                        .any(|m| m.value == Reading::Text("review".into()))
            })
        {
            break;
        }
    }
    assert_eq!(live.last_seq(), sequence);
}
