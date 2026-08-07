//! Exercised against real child processes, not mocks. A supervisor that only passes against
//! a fake process has not been tested at all.

use std::time::Duration;

use aos_core::{AgentId, AgentSpec, AgentState, RiskTier};
use aos_supervisor::Supervisor;

fn spec(id: &str, program: &str, args: &[&str]) -> AgentSpec {
    AgentSpec {
        id: AgentId::new(id).unwrap(),
        program: program.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        ceiling: RiskTier::Read,
    }
}

/// Each test gets its own log directory. The `TempDir` is returned alongside the supervisor
/// because dropping it deletes the directory out from under the running agents.
fn sleeper() -> (Supervisor, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let sup = Supervisor::new(
        [
            "/usr/bin/sleep".to_string(),
            "/usr/bin/true".to_string(),
            "/usr/bin/seq".to_string(),
            "/usr/bin/echo".to_string(),
        ],
        dir.path(),
    );
    (sup, dir)
}

#[test]
fn starts_lists_and_stops_a_real_process() {
    let (mut sup, _dir) = sleeper();
    let s = spec("sleeper", "/usr/bin/sleep", &["30"]);

    let handle = sup.start(&s).unwrap();
    assert!(handle.pid > 0);
    assert!(
        handle.start_token > 0,
        "a live process always has a start time"
    );
    assert_eq!(sup.list().len(), 1);

    let stopped = sup.stop(&s.id, Duration::from_secs(2)).unwrap();
    // Killed by a signal, so there is no exit code. That is why the field is an Option.
    assert!(matches!(stopped, AgentState::Stopped { .. }));
    assert!(sup.list().is_empty());
}

#[test]
fn a_program_off_the_allowlist_is_refused() {
    let (mut sup, _dir) = sleeper();
    let err = sup
        .start(&spec("shady", "/bin/sh", &["-c", "echo hi"]))
        .unwrap_err();
    assert!(err.to_string().contains("not an allowed program"));
}

/// Arguments reach execve as a list, so shell metacharacters are inert. This is the check
/// that would have caught a supervisor rewritten to build a command string.
#[test]
fn shell_metacharacters_in_arguments_stay_literal() {
    let (mut sup, _dir) = sleeper();
    let s = spec(
        "injected",
        "/usr/bin/sleep",
        &["0.05; touch /tmp/aos-pwned"],
    );

    // sleep rejects the argument, which is the point. It was passed as one opaque string.
    sup.start(&s).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(!std::path::Path::new("/tmp/aos-pwned").exists());
    let _ = sup.stop(&s.id, Duration::from_millis(200));
}

#[test]
fn starting_the_same_agent_twice_is_refused() {
    let (mut sup, _dir) = sleeper();
    let s = spec("dupe", "/usr/bin/sleep", &["30"]);
    sup.start(&s).unwrap();

    let err = sup.start(&s).unwrap_err();
    assert!(err.to_string().contains("already running"));

    sup.stop(&s.id, Duration::from_secs(2)).unwrap();
}

#[test]
fn an_exited_agent_reports_its_exit_code() {
    let (mut sup, _dir) = sleeper();
    let s = spec("quick", "/usr/bin/true", &[]);
    sup.start(&s).unwrap();

    // Poll rather than sleeping a fixed amount, so a slow machine does not fail the test.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match sup.state(&s.id) {
            Ok(AgentState::Stopped { code }) => {
                assert_eq!(code, Some(0));
                break;
            }
            _ if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            other => panic!("agent never exited, last state {other:?}"),
        }
    }
}

#[test]
fn stopping_an_unknown_agent_names_it() {
    let (mut sup, _dir) = sleeper();
    let id = AgentId::new("ghost").unwrap();
    let err = sup.stop(&id, Duration::from_secs(1)).unwrap_err();
    assert!(err.to_string().contains("ghost"));
}

/// Waits for an agent to exit, returning its code. Panics rather than hanging, because a
/// test that hangs tells you nothing about why.
fn wait_for_exit(sup: &mut Supervisor, id: &AgentId, limit: Duration) -> Option<i32> {
    let deadline = std::time::Instant::now() + limit;
    loop {
        match sup.state(id) {
            Ok(AgentState::Stopped { code }) => return code,
            _ if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20))
            }
            other => panic!("{id} never exited, last state {other:?}"),
        }
    }
}

/// Regression test for bug 1. Output went to a pipe nobody drained, so it was lost, and an
/// agent writing past the roughly 64 KB pipe buffer blocked forever.
///
/// `seq 1 200000` is about 1.3 MB, comfortably past the buffer. Against the old code this
/// test hangs until the deadline and then fails.
#[test]
fn a_chatty_agent_finishes_and_its_output_is_kept() {
    let (mut sup, _dir) = sleeper();
    let s = spec("chatty", "/usr/bin/seq", &["1", "200000"]);
    let log = sup.log_path(&s.id);

    sup.start(&s).unwrap();
    assert_eq!(
        wait_for_exit(&mut sup, &s.id, Duration::from_secs(20)),
        Some(0)
    );

    let written = std::fs::metadata(&log).unwrap().len();
    assert!(written > 1_000_000, "only {written} bytes reached {log:?}");
}

/// Output has to survive somewhere readable, otherwise debugging an agent means guessing.
#[test]
fn agent_output_lands_in_its_log() {
    let (mut sup, _dir) = sleeper();
    let s = spec("talker", "/usr/bin/echo", &["reporting in"]);
    let log = sup.log_path(&s.id);

    sup.start(&s).unwrap();
    wait_for_exit(&mut sup, &s.id, Duration::from_secs(5));

    assert_eq!(std::fs::read_to_string(&log).unwrap(), "reporting in\n");
}

/// Kill switch invariant. Every agent goes down, whatever its tier.
#[test]
fn stop_all_takes_down_every_agent() {
    let (mut sup, _dir) = sleeper();
    for name in ["one", "two", "three"] {
        sup.start(&spec(name, "/usr/bin/sleep", &["30"])).unwrap();
    }
    assert_eq!(sup.list().len(), 3);

    let stopped = sup.stop_all(Duration::from_secs(2));
    assert_eq!(stopped.len(), 3);
    assert!(stopped.iter().all(|(_, r)| r.is_ok()));
    assert!(sup.list().is_empty());
}
