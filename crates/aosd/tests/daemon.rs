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
