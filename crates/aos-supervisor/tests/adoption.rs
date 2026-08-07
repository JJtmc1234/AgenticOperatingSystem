//! Adopting agents that outlived their supervisor.
//!
//! Every test here makes a real orphan. A `sleep` is spawned by an intermediate process which
//! is then killed, so the sleeper is genuinely reparented and genuinely not our child. Nothing
//! about adoption is interesting against a process we could have waited on anyway.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use aos_core::{AgentId, AgentState, Event, ProcessHandle, Record};
use aos_supervisor::{Supervisor, proc};

fn id(name: &str) -> AgentId {
    AgentId::new(name).unwrap()
}

fn sup() -> (Supervisor, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let sup = Supervisor::new(["/usr/bin/sleep".to_string()], dir.path());
    (sup, dir)
}

/// Spawns a sleeper that is not our child, and returns its handle.
///
/// `setsid --fork` starts a new session, forks, and exits, so the sleeper it leaves behind is
/// reparented to init. That is the same situation as a supervisor being killed with its
/// agents still running.
///
/// The `--fork` is load bearing. Without it setsid only forks when it is not already a
/// process group leader, and otherwise execs `sleep` in place. Then the sleeper is still our
/// child and the wait below blocks for the full duration.
fn make_orphan(seconds: &str) -> ProcessHandle {
    let mut mid: Child = Command::new("/usr/bin/setsid")
        .args(["--fork", "/usr/bin/sleep", seconds])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // Bounded, so a setsid that did not fork fails the test in a second instead of hanging
    // for the whole sleep duration. See bug 2 in bug-list.md.
    wait_bounded(&mut mid, Duration::from_secs(5));

    // Find the sleeper by walking /proc, since setsid did not tell us its pid.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(pid) = find_orphan_sleeper(seconds) {
            return ProcessHandle {
                pid,
                start_token: proc::start_token(pid).unwrap(),
            };
        }
        assert!(Instant::now() < deadline, "no orphaned sleeper appeared");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits for the intermediate process, but never forever.
///
/// A test that hangs tells you nothing about why. This is the same rule the supervisor itself
/// follows in `signal::wait_bounded`, applied to the test helper that broke it.
fn wait_bounded(child: &mut Child, limit: Duration) {
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                panic!(
                    "setsid did not exit within {limit:?}, so it never forked and the sleeper \
                     is still our child rather than an orphan"
                );
            }
            Err(e) => panic!("could not wait on setsid: {e}"),
        }
    }
}

/// A `sleep` with our exact argument whose parent is init rather than us.
fn find_orphan_sleeper(seconds: &str) -> Option<u32> {
    for entry in std::fs::read_dir("/proc").ok()? {
        let entry = entry.ok()?;
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(cmdline) = std::fs::read_to_string(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        let args: Vec<_> = cmdline.split('\0').filter(|s| !s.is_empty()).collect();
        if args.len() == 2 && args[0].ends_with("sleep") && args[1] == seconds && is_orphan(pid) {
            return Some(pid);
        }
    }
    None
}

/// Parent pid is field 4 of `/proc/<pid>/stat`, read after the last paren for the same reason
/// the start token is.
fn is_orphan(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    let Some(cut) = stat.rfind(')') else {
        return false;
    };
    stat[cut + 1..]
        .split_whitespace()
        .nth(1)
        .and_then(|ppid| ppid.parse::<u32>().ok())
        .is_some_and(|ppid| ppid != std::process::id())
}

#[test]
fn adopts_a_real_orphan_and_reports_it_running() {
    let (mut s, _dir) = sup();
    let handle = make_orphan("511");

    s.adopt(id("orphan"), handle).unwrap();
    assert!(s.is_adopted(&id("orphan")));
    assert_eq!(
        s.state(&id("orphan")).unwrap(),
        AgentState::Running { pid: handle.pid }
    );

    s.stop(&id("orphan"), Duration::from_secs(3)).unwrap();
    assert!(!proc::is_still(handle), "the orphan should be gone");
}

/// The point of the whole exercise. A process we never spawned, stopped safely.
#[test]
fn stops_an_adopted_orphan_through_its_descriptor() {
    let (mut s, _dir) = sup();
    let handle = make_orphan("512");
    s.adopt(id("target"), handle).unwrap();

    let state = s.stop(&id("target"), Duration::from_secs(3)).unwrap();

    // No exit code, and that is correct. Init reaped it, so the code went there. Inventing a
    // zero here would be a lie that looks like success.
    assert_eq!(state, AgentState::Stopped { code: None });
    assert!(!proc::is_still(handle));
    assert!(s.list().is_empty());
}

/// The guard that makes adoption safe. A stale token means the pid may belong to anyone.
#[test]
fn refuses_to_adopt_a_handle_with_the_wrong_token() {
    let (mut s, _dir) = sup();
    let mut handle = make_orphan("513");
    let real = handle;
    handle.start_token += 1;

    let err = s.adopt(id("impostor"), handle).unwrap_err();
    assert!(err.to_string().contains("will not be adopted"));
    assert!(s.list().is_empty());

    // And the real process was left completely alone.
    assert!(proc::is_still(real));
    let _ = aos_supervisor::PidFd::open(real).map(|p| p.send(libc::SIGKILL));
}

#[test]
fn refuses_to_adopt_a_process_that_is_gone() {
    let (mut s, _dir) = sup();
    let err = s
        .adopt(
            id("ghost"),
            ProcessHandle {
                pid: u32::MAX,
                start_token: 1,
            },
        )
        .unwrap_err();
    assert!(err.to_string().contains("will not be adopted"));
}

/// Replay drives adoption, so a log naming a live orphan should end with it tracked.
#[test]
fn adopt_from_a_log_picks_up_the_survivor_and_names_the_lost() {
    let (mut s, _dir) = sup();
    let alive = make_orphan("514");

    let records = vec![
        Record {
            seq: 1,
            at: 1,
            agent: id("survivor"),
            event: Event::Started {
                handle: alive,
                program: "/usr/bin/sleep".into(),
            },
        },
        Record {
            seq: 2,
            at: 2,
            agent: id("casualty"),
            event: Event::Started {
                handle: ProcessHandle {
                    pid: u32::MAX,
                    start_token: 1,
                },
                program: "/usr/bin/sleep".into(),
            },
        },
    ];

    let recovered = s.adopt_from(&records);

    assert_eq!(recovered.alive.len(), 1);
    assert_eq!(recovered.lost.len(), 1);
    assert_eq!(recovered.lost[0].0, id("casualty"));
    assert!(s.is_adopted(&id("survivor")));

    s.stop(&id("survivor"), Duration::from_secs(3)).unwrap();
}
