//! The worker, checked against a stub daemon rather than a real one.
//!
//! A `UnixListener` on the socket path the client already looks for is enough to exercise the
//! whole round trip: the real request goes out over the real transport and a real response
//! comes back. Nothing here starts `aosd`, and nothing here can start a process.

use aos_core::{PlanId, Request, Response, Verdict};

use super::calls::{listed, load_spec, ping, run};
use super::*;
use crate::link::Link;

fn spec(id: &str, ceiling: RiskTier) -> AgentSpec {
    AgentSpec {
        id: AgentId::new(id).unwrap(),
        // A program that really is at the tier named. The verdict comes from the program now,
        // so a spec that only claims a tier is judged at whatever its program actually is, and
        // claiming one was the whole of bug 34. Nothing here runs a process: the daemon on the
        // other end of these tests is a stub.
        program: match ceiling {
            RiskTier::Read => "/usr/bin/echo",
            RiskTier::Write => "/usr/bin/mkdir",
            RiskTier::System => "/usr/bin/chmod",
            RiskTier::Destructive => "/usr/bin/rm",
        }
        .into(),
        args: vec!["hello".into()],
        ceiling,
    }
}

/// The worker's own ping is unsolicited and must not settle a stop somebody is waiting on.
/// Everything else answers a person.
#[test]
fn a_heartbeat_does_not_settle_an_order_it_is_not_the_answer_to() {
    let stopping = Order::Stop {
        agent: AgentId::new("brief").unwrap(),
        grace_secs: 5,
    };
    assert!(!Outcome::Heartbeat(Link::Unknown).settles(Some(&stopping)));
    assert!(!Outcome::Heartbeat(Link::Unknown).settles(None));
    assert!(Outcome::good("done").settles(Some(&stopping)));
    assert!(
        Outcome::PlanOffered {
            plan: PlanId::quoted("abc"),
            agent: AgentId::new("brief").unwrap(),
            tier: RiskTier::Destructive,
            summary: "would run rm".into(),
        }
        .settles(Some(&stopping))
    );
}

/// And the case that wedged the panel. `Ping` is the one order whose whole reply is a
/// heartbeat, so if a heartbeat cannot settle it, nothing can and the slot is never returned.
#[test]
fn a_heartbeat_settles_a_ping_because_that_is_the_whole_of_its_answer() {
    assert!(Outcome::Heartbeat(Link::Unknown).settles(Some(&Order::Ping)));

    // The reply set really does contain nothing else, which is what makes the case above the
    // only way out. Checked here rather than assumed, since adding a note to `Order::Ping`
    // later would make this test the one that says the guard is no longer load bearing.
    let dir = tempfile::tempdir().unwrap();
    let reply = run(dir.path(), Order::Ping);
    assert!(
        reply.iter().all(|o| matches!(o, Outcome::Heartbeat(_))),
        "{reply:?}"
    );
}

/// With no daemon there, an order must come back as a refusal and as a link state, not as
/// a hang and not as silence.
#[test]
fn an_order_with_no_daemon_answers_and_says_the_daemon_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let out = run(
        dir.path(),
        Order::Stop {
            agent: AgentId::new("brief").unwrap(),
            grace_secs: 5,
        },
    );
    assert_eq!(out.len(), 2, "an answer and a link state");
    assert!(matches!(
        out[0],
        Outcome::Note {
            tone: Tone::Bad,
            ..
        }
    ));
    match &out[1] {
        Outcome::Heartbeat(Link::Silent { why }) => {
            assert!(why.contains("no daemon answering"), "{why}")
        }
        other => panic!("expected a silent link, got {other:?}"),
    }
}

#[test]
fn a_ping_with_no_daemon_is_silent_rather_than_unknown() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(ping(dir.path()), Link::Silent { .. }));
}

#[test]
fn a_spec_is_read_from_disk_with_the_verdict_the_policy_gives_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brief.json");
    std::fs::write(
        &path,
        serde_json::to_string(&spec("brief", RiskTier::Destructive)).unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("policy.toml"),
        "plan_ttl_secs = 120\n\
         [tiers]\nread = \"allow\"\nwrite = \"prompt\"\n\
         system = \"prompt\"\ndestructive = \"prompt\"\n",
    )
    .unwrap();

    match load_spec(dir.path(), &path) {
        Outcome::SpecLoaded { spec, verdict } => {
            assert_eq!(spec.id.as_str(), "brief");
            assert_eq!(verdict, Some(Verdict::Prompt));
        }
        other => panic!("expected a loaded spec, got {other:?}"),
    }
}

/// No policy file means the panel does not know what the daemon will do. It says so.
#[test]
fn with_no_policy_file_the_verdict_is_not_claimed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("brief.json");
    std::fs::write(
        &path,
        serde_json::to_string(&spec("brief", RiskTier::Read)).unwrap(),
    )
    .unwrap();
    match load_spec(dir.path(), &path) {
        Outcome::SpecLoaded { verdict, .. } => assert_eq!(verdict, None),
        other => panic!("expected a loaded spec, got {other:?}"),
    }
}

#[test]
fn a_spec_that_will_not_parse_is_reported_rather_than_assumed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.json");
    std::fs::write(&path, "{ this is not a spec }").unwrap();
    match load_spec(dir.path(), &path) {
        Outcome::Note {
            tone: Tone::Bad,
            text,
        } => {
            assert!(text.contains("not a valid agent spec"), "{text}")
        }
        other => panic!("expected a complaint, got {other:?}"),
    }
}

#[test]
fn stopping_nothing_reads_as_nothing_rather_than_as_an_empty_list() {
    assert!(listed(&[]).contains("nothing was running"));
    assert_eq!(
        listed(&[AgentId::new("a").unwrap(), AgentId::new("b").unwrap()]),
        "a, b"
    );
}

/// A stand in for `aosd`: one socket, canned answers, and no processes anywhere.
///
/// Enough to exercise the whole round trip, because the client, the wire format and the
/// responses are all the real ones. What it deliberately cannot do is start or stop anything,
/// which is why it is safe to run in a test suite.
fn stub_daemon(dir: &std::path::Path, connections: usize) -> std::thread::JoinHandle<()> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    let listener = UnixListener::bind(aos_cli::client::socket_path(dir)).unwrap();
    std::thread::spawn(move || {
        for _ in 0..connections {
            let (stream, _) = listener.accept().unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).unwrap();
            let response = answer(serde_json::from_str::<Request>(&line).unwrap());
            writeln!(writer, "{}", serde_json::to_string(&response).unwrap()).unwrap();
            writer.flush().unwrap();
        }
    })
}

fn answer(request: Request) -> Response {
    match request {
        Request::Ping => Response::Pong {
            version: "stub".into(),
            tracking: 1,
        },
        Request::List => Response::Agents { agents: vec![] },
        Request::Stop { agent, .. } => Response::Stopped {
            agent,
            state: aos_core::AgentState::Stopped { code: Some(0) },
        },
        Request::StopAll { .. } => Response::StoppedAll {
            stopped: vec![AgentId::new("brief").unwrap()],
            failed: vec![],
        },
        // The handshake, exactly as the daemon does it: no commit quoted means a plan and
        // nothing else, and a commit is what starts anything.
        Request::Start { spec, commit: None } => Response::PlanRequired {
            plan: PlanId::quoted("4f2a"),
            agent: spec.id.clone(),
            tier: spec.ceiling,
            summary: format!(
                "{} would run {} at tier {}",
                spec.id, spec.program, spec.ceiling
            ),
        },
        Request::Start { .. } => Response::Started {
            handle: aos_core::ProcessHandle {
                pid: 4242,
                start_token: 991,
                // The panel only ever reads a handle, so which boot it came from does not
                // matter here.
                boot: None,
            },
        },
    }
}

#[test]
fn a_stop_that_a_daemon_answers_comes_back_as_a_result_and_a_live_link() {
    let dir = tempfile::tempdir().unwrap();
    let served = stub_daemon(dir.path(), 2);

    let out = run(
        dir.path(),
        Order::Stop {
            agent: AgentId::new("brief").unwrap(),
            grace_secs: 5,
        },
    );
    served.join().unwrap();

    match &out[0] {
        Outcome::Note { text, tone } => {
            assert_eq!(*tone, Tone::Good);
            assert!(text.contains("stopped brief"), "{text}");
            assert!(text.contains("exit 0"), "{text}");
        }
        other => panic!("expected a result, got {other:?}"),
    }
    // The link is refreshed by the same round trip, so a panel that just stopped something
    // cannot still be showing NO DAEMON.
    match &out[1] {
        Outcome::Heartbeat(Link::Answering { version, tracking }) => {
            assert_eq!(version, "stub");
            assert_eq!(*tracking, 1);
        }
        other => panic!("expected a live link, got {other:?}"),
    }
}

/// Harness invariant, checked over the real wire. The planning call quotes no commit and gets
/// a plan back, and only the call that quotes that plan starts anything.
#[test]
fn the_planning_call_runs_nothing_and_the_committing_call_is_a_second_request() {
    let dir = tempfile::tempdir().unwrap();
    let served = stub_daemon(dir.path(), 4);

    let spec = Box::new(spec("risky", RiskTier::Destructive));
    let planned = run(dir.path(), Order::Plan(spec.clone()));
    let plan = match &planned[0] {
        Outcome::PlanOffered {
            plan,
            agent,
            tier,
            summary,
        } => {
            assert_eq!(agent.as_str(), "risky");
            assert_eq!(*tier, RiskTier::Destructive);
            assert!(summary.contains("would run /usr/bin/rm"), "{summary}");
            plan.clone()
        }
        other => panic!("expected a plan and nothing started, got {other:?}"),
    };

    let committed = run(dir.path(), Order::Commit { spec, plan });
    served.join().unwrap();

    match &committed[0] {
        Outcome::Note { text, tone } => {
            assert_eq!(*tone, Tone::Good);
            assert!(text.contains("committed, running as pid 4242"), "{text}");
        }
        other => panic!("expected a start, got {other:?}"),
    }
}

/// A refusal from the daemon is shown as a refusal, not as a crash and not as a success.
#[test]
fn a_refusal_from_the_daemon_reaches_the_screen() {
    let dir = tempfile::tempdir().unwrap();
    let path = aos_cli::client::socket_path(dir.path());
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let served = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        for _ in 0..2 {
            let (stream, _) = listener.accept().unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).unwrap();
            let response = match serde_json::from_str::<Request>(&line).unwrap() {
                Request::Ping => Response::Pong {
                    version: "stub".into(),
                    tracking: 0,
                },
                _ => Response::error("policy denies risky at tier destructive"),
            };
            writeln!(writer, "{}", serde_json::to_string(&response).unwrap()).unwrap();
            writer.flush().unwrap();
        }
    });

    let out = run(
        dir.path(),
        Order::Commit {
            spec: Box::new(spec("risky", RiskTier::Destructive)),
            plan: PlanId::quoted("4f2a"),
        },
    );
    served.join().unwrap();

    match &out[0] {
        Outcome::Note { text, tone } => {
            assert_eq!(*tone, Tone::Bad);
            assert!(text.contains("policy denies"), "{text}");
        }
        other => panic!("expected the refusal, got {other:?}"),
    }
}
