//! Real handoff processes with deterministic local agents and no mail access.
use std::process::Command;

fn run(work: &str) -> (tempfile::TempDir, std::process::Output) {
    let home = tempfile::tempdir().unwrap();
    carl::army::personnel::found(home.path(), 1).unwrap();
    let bin = env!("CARGO_BIN_EXE_carl");
    let fixtures = format!("{}/tests/stand-in", env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(bin)
        .args([
            "--home",
            home.path().to_str().unwrap(),
            "handoff",
            "--from",
            "carl",
            "--to",
            "olivia",
            work,
        ])
        .env("PATH", format!("{fixtures}:/usr/bin:/bin"))
        .env("AOS_TEST_HOME", home.path())
        .env("AOS_TEST_CARL", bin)
        .env("AOS_TEST_TRACE", home.path().join("trace"))
        .output()
        .unwrap();
    (home, out)
}

#[test]
fn carl_olivia_miles_handoffs_return_the_workers_result() {
    let (home, out) = run("check synthetic inbox");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "Olivia reviewed: Miles checked synthetic inbox: fixture-42"
    );
    let trace = std::fs::read_to_string(home.path().join("trace")).unwrap();
    assert_eq!(
        trace,
        "olivia:check synthetic inbox\nmiles:check synthetic inbox\n"
    );
}

#[test]
fn miles_failure_reaches_carl_as_failure() {
    let (home, out) = run("fail fixture");
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Miles fixture failure"));
    let activity = std::fs::read_to_string(carl::army::watching::path(home.path())).unwrap();
    for who in ["olivia", "miles"] {
        assert!(
            activity.lines().any(|line| {
                let v: serde_json::Value = serde_json::from_str(line).unwrap();
                v["agent"] == who && v["kind"] == "answered" && v["interrupted"] == true
            }),
            "{activity}"
        );
    }
}
