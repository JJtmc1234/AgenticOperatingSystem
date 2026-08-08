//! Drives the real `aosd` binary over a real socket.
//!
//! Not the `Daemon` struct in isolation. The things worth testing here are the socket, the
//! protocol and the restart, and none of those exist until the binary is actually running.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A running daemon that is killed when the test ends, however it ends.
struct Aosd {
    child: Child,
    run_dir: PathBuf,
}

impl Aosd {
    fn start(run_dir: &Path) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_aosd"))
            .args(["--run-dir", run_dir.to_str().unwrap()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let daemon = Self {
            child,
            run_dir: run_dir.to_path_buf(),
        };
        daemon.wait_until_listening();
        daemon
    }

    fn socket(&self) -> PathBuf {
        self.run_dir.join("aosd.sock")
    }

    fn wait_until_listening(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if UnixStream::connect(self.socket()).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("aosd never started listening on {:?}", self.socket());
    }

    /// One request, one response, as raw JSON text.
    fn ask(&self, request: &str) -> String {
        self.try_ask(request)
            .unwrap_or_else(|| panic!("no answer to {request}"))
    }

    /// Same, but never panics. Used from `Drop`, which can run while a test is already
    /// unwinding, and a panic there would abort the process and hide the real failure.
    fn try_ask(&self, request: &str) -> Option<String> {
        let stream = UnixStream::connect(self.socket()).ok()?;
        let mut writer = stream.try_clone().ok()?;
        writeln!(writer, "{request}").ok()?;
        writer.flush().ok()?;

        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).ok()?;
        Some(line.trim().to_string())
    }

    /// Stops the daemon the way a person would, leaving its agents running.
    fn shutdown(&mut self) {
        let _ = Command::new("kill")
            .args(["-TERM", &self.child.id().to_string()])
            .status();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
    }
}

impl Drop for Aosd {
    fn drop(&mut self) {
        // Stop the agents before the daemon, not after. Killing the daemon first leaves them
        // running, which is correct in production and a process leak in a test. A failing
        // test must not litter the machine with sleepers.
        let _ = self.try_ask(r#"{"request":"stop_all"}"#);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn run_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("allowed-programs.json"),
        r#"["/usr/bin/sleep","/usr/bin/echo"]"#,
    )
    .unwrap();
    dir
}

/// A run directory with a policy in it.
fn run_dir_with_policy(policy: &str) -> tempfile::TempDir {
    let dir = run_dir();
    std::fs::write(dir.path().join("policy.toml"), policy).unwrap();
    dir
}

const PROMPT_ON_DESTRUCTIVE: &str = r#"
[tiers]
read = "allow"
write = "prompt"
system = "prompt"
destructive = "prompt"

[agents]
"wiper" = "deny"
"#;

/// A start request at a given tier, optionally quoting a plan.
fn start_at(id: &str, secs: &str, tier: &str, commit: Option<&str>) -> String {
    let commit = commit
        .map(|c| format!(r#","commit":"{c}""#))
        .unwrap_or_default();
    format!(
        r#"{{"request":"start","spec":{{"id":"{id}","program":"/usr/bin/sleep","args":["{secs}"],"ceiling":"{tier}"}}{commit}}}"#
    )
}

fn plan_id_of(answer: &str) -> String {
    let value: serde_json::Value =
        serde_json::from_str(answer).unwrap_or_else(|e| panic!("not JSON: {answer} ({e})"));
    value["plan"]
        .as_str()
        .unwrap_or_else(|| panic!("no plan id in {answer}"))
        .to_string()
}

fn sleeper(id: &str) -> String {
    format!(
        r#"{{"request":"start","spec":{{"id":"{id}","program":"/usr/bin/sleep","args":["400"],"ceiling":"read"}}}}"#
    )
}

/// Pulls the pid out of a `started` response.
///
/// The handle is a nested object rather than flattened, so the path is `handle.pid`. Asserted
/// rather than assumed, because reading the wrong field yields `None` and a panic that says
/// nothing about which field was missing.
fn pid_of(started: &str) -> u32 {
    let value: serde_json::Value =
        serde_json::from_str(started).unwrap_or_else(|e| panic!("not JSON: {started} ({e})"));
    value["handle"]["pid"]
        .as_u64()
        .unwrap_or_else(|| panic!("no handle.pid in {started}")) as u32
}

fn is_running(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[test]
fn answers_a_ping() {
    let dir = run_dir();
    let daemon = Aosd::start(dir.path());
    assert!(daemon.ask(r#"{"request":"ping"}"#).contains("\"pong\""));
}

/// The socket lets anyone who reaches it start processes as this user, so it must not be
/// reachable by anyone else.
#[test]
fn the_socket_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let dir = run_dir();
    let daemon = Aosd::start(dir.path());
    let mode = std::fs::metadata(daemon.socket())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn starts_an_agent_and_lists_it() {
    let dir = run_dir();
    let daemon = Aosd::start(dir.path());

    let started = daemon.ask(&sleeper("worker"));
    assert!(started.contains("\"started\""), "{started}");
    assert!(is_running(pid_of(&started)));

    let listed = daemon.ask(r#"{"request":"list"}"#);
    assert!(listed.contains("worker"), "{listed}");
    assert!(listed.contains("\"adopted\":false"), "{listed}");

    daemon.ask(r#"{"request":"stop_all"}"#);
}

/// The acceptance criterion for this phase. An agent must outlive its daemon and be taken
/// back, not lost and not duplicated.
#[test]
fn a_restarted_daemon_readopts_its_surviving_agent() {
    let dir = run_dir();
    let mut first = Aosd::start(dir.path());
    let pid = pid_of(&first.ask(&sleeper("survivor")));

    first.shutdown();
    assert!(is_running(pid), "the agent must outlive its daemon");

    let second = Aosd::start(dir.path());
    let listed = second.ask(r#"{"request":"list"}"#);

    assert!(listed.contains("survivor"), "{listed}");
    assert!(
        listed.contains("\"adopted\":true"),
        "it should be marked inherited: {listed}"
    );
    assert_eq!(
        listed.matches("survivor").count(),
        1,
        "adopted once, not twice: {listed}"
    );

    second.ask(r#"{"request":"stop_all"}"#);
    assert!(!is_running(pid));
}

#[test]
fn the_kill_switch_stops_everything() {
    let dir = run_dir();
    let daemon = Aosd::start(dir.path());

    let pids: Vec<u32> = ["one", "two", "three"]
        .iter()
        .map(|id| pid_of(&daemon.ask(&sleeper(id))))
        .collect();
    assert!(pids.iter().all(|p| is_running(*p)));

    let stopped = daemon.ask(r#"{"request":"stop_all"}"#);
    assert!(stopped.contains("\"failed\":[]"), "{stopped}");

    for pid in pids {
        assert!(!is_running(pid), "pid {pid} survived the kill switch");
    }
}

/// A program off the allowlist is refused, and the refusal is written down.
#[test]
fn a_refused_start_is_answered_and_recorded() {
    let dir = run_dir();
    let daemon = Aosd::start(dir.path());

    let refused = daemon.ask(
        r#"{"request":"start","spec":{"id":"shady","program":"/bin/sh","args":["-c","echo hi"],"ceiling":"read"}}"#,
    );
    assert!(refused.contains("not an allowed program"), "{refused}");

    let log = std::fs::read_to_string(dir.path().join("events.jsonl")).unwrap();
    assert!(
        log.contains("\"refused\""),
        "the refusal must be in the log"
    );
}

/// Nonsense must come back as an answer. Silence cannot be told apart from a crash.
#[test]
fn malformed_requests_are_answered_rather_than_dropped() {
    let dir = run_dir();
    let daemon = Aosd::start(dir.path());

    for bad in [
        "not json at all",
        r#"{"request":"rm_rf"}"#,
        r#"{"request":"stop","agent":"../../etc/passwd"}"#,
    ] {
        let answer = daemon.ask(bad);
        assert!(answer.contains("\"error\""), "{bad} produced {answer}");
    }

    // And the daemon is still healthy afterwards.
    assert!(daemon.ask(r#"{"request":"ping"}"#).contains("\"pong\""));
}

/// Two daemons on one run directory would mean two owners of the same agents.
#[test]
fn a_second_daemon_refuses_to_share_the_socket() {
    let dir = run_dir();
    let _first = Aosd::start(dir.path());

    let second = Command::new(env!("CARGO_BIN_EXE_aosd"))
        .args(["--run-dir", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!second.status.success(), "the second daemon should refuse");
    assert!(
        String::from_utf8_lossy(&second.stderr).contains("already listening"),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
}

/// Read runs on its own. That is the only tier that may.
#[test]
fn a_read_tier_agent_needs_no_commit() {
    let dir = run_dir_with_policy(PROMPT_ON_DESTRUCTIVE);
    let daemon = Aosd::start(dir.path());

    let answer = daemon.ask(&start_at("reader", "400", "read", None));
    assert!(answer.contains("\"started\""), "{answer}");
}

/// The acceptance criterion for this phase. Destructive must not act in one step.
#[test]
fn a_destructive_agent_will_not_run_without_a_commit() {
    let dir = run_dir_with_policy(PROMPT_ON_DESTRUCTIVE);
    let daemon = Aosd::start(dir.path());

    let answer = daemon.ask(&start_at("destroyer", "400", "destructive", None));
    assert!(answer.contains("\"plan_required\""), "{answer}");

    // And nothing is running, which is the part that actually matters.
    let listed = daemon.ask(r#"{"request":"list"}"#);
    assert!(!listed.contains("destroyer"), "{listed}");

    // The refusal is in the log with a reason, as the criterion requires.
    let log = std::fs::read_to_string(dir.path().join("events.jsonl")).unwrap();
    assert!(log.contains("\"planned\""), "{log}");
}

/// The whole reason plans record their request. Agreeing to one thing must not
/// authorise another.
#[test]
fn a_plan_cannot_be_committed_against_a_different_request() {
    let dir = run_dir_with_policy(PROMPT_ON_DESTRUCTIVE);
    let daemon = Aosd::start(dir.path());

    let plan = plan_id_of(&daemon.ask(&start_at("destroyer", "400", "destructive", None)));

    // Same agent, same tier, different arguments.
    let swapped = daemon.ask(&start_at("destroyer", "999", "destructive", Some(&plan)));
    assert!(swapped.contains("different request"), "{swapped}");

    let listed = daemon.ask(r#"{"request":"list"}"#);
    assert!(
        !listed.contains("destroyer"),
        "nothing should have run: {listed}"
    );

    // The honest commit still works, so the refusal did not burn the plan.
    let honest = daemon.ask(&start_at("destroyer", "400", "destructive", Some(&plan)));
    assert!(honest.contains("\"started\""), "{honest}");
}

#[test]
fn a_plan_cannot_be_used_twice() {
    let dir = run_dir_with_policy(PROMPT_ON_DESTRUCTIVE);
    let daemon = Aosd::start(dir.path());

    let plan = plan_id_of(&daemon.ask(&start_at("once", "400", "destructive", None)));
    assert!(
        daemon
            .ask(&start_at("once", "400", "destructive", Some(&plan)))
            .contains("\"started\"")
    );

    daemon.ask(r#"{"request":"stop","agent":"once"}"#);

    let replayed = daemon.ask(&start_at("once", "400", "destructive", Some(&plan)));
    assert!(replayed.contains("no plan"), "{replayed}");
}

#[test]
fn an_invented_plan_id_is_refused_and_logged() {
    let dir = run_dir_with_policy(PROMPT_ON_DESTRUCTIVE);
    let daemon = Aosd::start(dir.path());

    let answer = daemon.ask(&start_at(
        "forger",
        "400",
        "destructive",
        Some("00000000000000000000000000000000"),
    ));
    assert!(answer.contains("no plan"), "{answer}");

    let log = std::fs::read_to_string(dir.path().join("events.jsonl")).unwrap();
    assert!(log.contains("\"refused\""), "{log}");
}

/// A denied agent is denied whatever its tier, and no plan is offered.
#[test]
fn a_denied_agent_is_never_offered_a_plan() {
    let dir = run_dir_with_policy(PROMPT_ON_DESTRUCTIVE);
    let daemon = Aosd::start(dir.path());

    let answer = daemon.ask(&start_at("wiper", "400", "read", None));
    assert!(answer.contains("policy denies"), "{answer}");
    assert!(
        !answer.contains("plan"),
        "deny must not hand out a plan: {answer}"
    );
}

/// With no policy file the safe default applies, so only read runs freely.
#[test]
fn the_default_policy_prompts_above_read() {
    let dir = run_dir();
    let daemon = Aosd::start(dir.path());

    assert!(
        daemon
            .ask(&start_at("reader", "400", "read", None))
            .contains("\"started\"")
    );
    assert!(
        daemon
            .ask(&start_at("writer", "400", "write", None))
            .contains("\"plan_required\"")
    );
}

/// A broken policy must stop the daemon rather than let it run under rules nobody wrote.
#[test]
fn a_malformed_policy_refuses_to_start_the_daemon() {
    let dir = run_dir_with_policy("this is not valid toml {{{");

    let out = Command::new(env!("CARGO_BIN_EXE_aosd"))
        .args(["--run-dir", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not a valid policy"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A policy that forgets a tier is refused too. Every meaning a missing tier could have
/// is a bad one.
#[test]
fn a_policy_missing_a_tier_refuses_to_start_the_daemon() {
    let dir = run_dir_with_policy("[tiers]\nread = \"allow\"\n");

    let out = Command::new(env!("CARGO_BIN_EXE_aosd"))
        .args(["--run-dir", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("destructive"), "{stderr}");
}
