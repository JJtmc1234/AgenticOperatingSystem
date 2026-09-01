//! The state, checked without a window.
//!
//! Everything here drives the real `App` against a real run directory. No daemon is started,
//! which is also the state the panel spends most of its life in.

use std::path::Path;

use aos_core::{AgentId, AgentSpec, Event, PlanId, ProcessHandle, Record, RiskTier};

use super::*;
use crate::rows::{Ending, Life};
use crate::worker::{Order, Outcome, Tone};

fn id(name: &str) -> AgentId {
    AgentId::new(name).unwrap()
}

fn record(seq: u64, agent: &str, event: Event) -> Record {
    Record {
        seq,
        at: 1_700_000_000 + seq,
        agent: id(agent),
        event,
    }
}

fn started(pid: u32) -> Event {
    Event::Started {
        handle: ProcessHandle {
            pid,
            start_token: pid as u64 * 7,
            // The panel only ever reads a handle, so which boot it came from does not matter here.
            boot: None,
        },
        program: "/usr/bin/sleep".into(),
    }
}

fn write_ledger(dir: &Path, records: &[Record]) {
    let text: String = records
        .iter()
        .map(|r| format!("{}\n", serde_json::to_string(r).unwrap()))
        .collect();
    std::fs::write(dir.join("events.jsonl"), text).unwrap();
}

fn spec(id_: &str, ceiling: RiskTier) -> Box<AgentSpec> {
    Box::new(AgentSpec {
        id: id(id_),
        program: "/usr/bin/echo".into(),
        args: vec!["hello".into()],
        ceiling,
    })
}

/// A run directory with no ledger in it is a normal state, not a fault. It has to read
/// differently from a ledger that exists and is empty.
#[test]
fn a_run_dir_with_no_ledger_is_empty_and_says_the_file_is_not_there() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    app.tick();

    assert!(app.rows().is_empty());
    assert!(app.records().is_empty());
    assert_eq!(app.running(), 0);
    assert_eq!(app.refusals(), 0);
    assert!(!app.ledger.exists, "there is no file");
    assert!(app.ledger.read_at.is_some(), "and reading it still worked");
    assert!(app.ledger.error.is_none());
    // The worker pings the moment it starts, so by now the link is either still unasked or
    // already known to be silent. What must never happen is it claiming a daemon.
    assert!(!app.link.can_command(), "there is no daemon here");
}

#[test]
fn an_empty_ledger_file_reads_as_empty_and_present() {
    let dir = tempfile::tempdir().unwrap();
    write_ledger(dir.path(), &[]);
    let mut app = App::new(dir.path().to_path_buf());
    app.tick();

    assert!(app.rows().is_empty());
    assert!(
        app.ledger.exists,
        "the file is there, it just has nothing in it"
    );
    assert!(app.ledger.error.is_none());
}

#[test]
fn the_ledger_folds_into_rows_and_the_feed_keeps_up_with_appends() {
    let dir = tempfile::tempdir().unwrap();
    write_ledger(dir.path(), &[record(1, "brief", started(4242))]);

    let mut app = App::new(dir.path().to_path_buf());
    app.tick();
    assert_eq!(app.rows().len(), 1);
    assert!(app.rows()[0].life.is_running());
    assert_eq!(app.running(), 1);
    assert_eq!(app.last_seq(), 1);

    write_ledger(
        dir.path(),
        &[
            record(1, "brief", started(4242)),
            record(2, "brief", Event::Exited { code: Some(0) }),
        ],
    );
    app.tick();

    assert_eq!(app.rows().len(), 1, "one agent, two records");
    assert_eq!(
        app.rows()[0].life,
        Life::Ended {
            how: Ending::Exited,
            code: Some(0),
            at: 1_700_000_002
        }
    );
    assert_eq!(app.running(), 0);
    assert_eq!(app.records().len(), 2);
    assert_eq!(app.last_seq(), 2);
}

/// A refusal has to be findable from the top of the screen, not only by scrolling the feed.
#[test]
fn a_refusal_reaches_both_the_count_and_the_row() {
    let dir = tempfile::tempdir().unwrap();
    write_ledger(
        dir.path(),
        &[
            record(1, "brief", started(1)),
            record(
                2,
                "blocked",
                Event::Refused {
                    reason: "/bin/rm is not on the allowlist".into(),
                },
            ),
        ],
    );
    let mut app = App::new(dir.path().to_path_buf());
    app.tick();

    assert_eq!(app.refusals(), 1);
    let refused = app.rows().iter().find(|r| r.id == id("blocked")).unwrap();
    assert_eq!(refused.refusals, 1);
    assert_eq!(refused.life.word(), "REFUSED");
    assert!(crate::format::detail(&refused.last.event).contains("allowlist"));
}

/// A shorter ledger was replaced rather than appended to. Everything held from before is
/// history of a different file, and the panel has to say so rather than merge the two.
#[test]
fn a_replaced_ledger_starts_the_history_again_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    write_ledger(
        dir.path(),
        &[
            record(1, "brief", started(1)),
            record(2, "brief", Event::Exited { code: Some(0) }),
            record(3, "other", started(2)),
        ],
    );
    let mut app = App::new(dir.path().to_path_buf());
    app.tick();
    assert_eq!(app.records().len(), 3);
    assert_eq!(app.rows().len(), 2);

    write_ledger(dir.path(), &[record(1, "fresh", started(9))]);
    app.tick();

    assert_eq!(app.ledger.restarts, 1);
    assert_eq!(app.records().len(), 1, "the old records are not kept");
    assert_eq!(app.rows().len(), 1);
    assert_eq!(app.rows()[0].id, id("fresh"));
    assert!(
        app.notes.iter().any(|n| n.text.contains("replaced")),
        "the person looking at it has to be told"
    );
}

/// The whole point of the panel is that it cannot damage the thing it is watching.
///
/// Checked by taking the write permission away from the run directory and its ledger, which
/// makes any attempted write fail rather than pass unnoticed, and then by comparing the bytes
/// and the modification time either side of a run that ticks and sends every command.
#[test]
fn nothing_the_panel_does_writes_to_the_ledger() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("events.jsonl");
    write_ledger(
        dir.path(),
        &[
            record(1, "brief", started(1)),
            record(2, "brief", Event::Exited { code: Some(0) }),
        ],
    );

    let before_bytes = std::fs::read(&ledger).unwrap();
    let before_mtime = std::fs::metadata(&ledger).unwrap().modified().unwrap();
    let before_listing = listing(dir.path());

    std::fs::set_permissions(&ledger, std::fs::Permissions::from_mode(0o444)).unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();

    // Somewhere writable to keep the spec, since the run directory is about to lose its write
    // permission and the spec is a file a person points the panel at, not part of the run dir.
    let elsewhere = tempfile::tempdir().unwrap();
    let spec_file = elsewhere.path().join("worker.json");
    std::fs::write(
        &spec_file,
        serde_json::to_string(&AgentSpec {
            id: id("brief"),
            program: "/usr/bin/sleep".into(),
            args: vec!["1".into()],
            ceiling: RiskTier::Read,
        })
        .unwrap(),
    )
    .unwrap();

    let mut app = App::new(dir.path().to_path_buf());
    for _ in 0..3 {
        app.tick();
    }

    // Every command a person can reach, one at a time and each one waited for. With no daemon
    // they all fail, and failing is not a licence to write anything down here: the ledger
    // belongs to the daemon.
    //
    // Waited for, because `dispatch` refuses to send while an order is outstanding. The old
    // version called all six in a row, so `ping` took the slot, the next three were dropped at
    // that guard, and `request_plan` and `commit` returned before reaching `dispatch` at all.
    // One of six actually ran, and the test would have kept passing if any of the other five
    // had started writing to the ledger. See bug 31.
    app.ping();
    settle(&mut app, "ping");

    app.stop(id("brief"));
    settle(&mut app, "stop");

    app.stop_all();
    settle(&mut app, "stop all");

    app.spec_path = spec_file.display().to_string();
    app.load_spec();
    settle(&mut app, "load spec");
    assert!(
        matches!(app.start, StartFlow::Loaded { .. }),
        "the spec has to load, or request_plan below returns before it sends anything"
    );

    app.request_plan();
    settle(&mut app, "request plan");

    // Put in by hand, because the only thing that reaches this state is a daemon offering a
    // plan and there is deliberately no daemon here. Without it `commit` returns at its own
    // guard and the one command that can start a process goes unexercised.
    app.start = StartFlow::Offered {
        spec: Box::new(AgentSpec {
            id: id("brief"),
            program: "/usr/bin/sleep".into(),
            args: vec!["1".into()],
            ceiling: RiskTier::Read,
        }),
        plan: PlanId::quoted("never-offered"),
        agent: id("brief"),
        tier: RiskTier::Read,
        summary: "would run sleep".into(),
    };
    app.commit();
    settle(&mut app, "commit");

    // Put the permissions back before asserting, so a failure does not leave an undeletable
    // directory behind in the temporary area.
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::set_permissions(&ledger, std::fs::Permissions::from_mode(0o644)).unwrap();

    assert_eq!(
        std::fs::read(&ledger).unwrap(),
        before_bytes,
        "the ledger changed"
    );
    assert_eq!(
        std::fs::metadata(&ledger).unwrap().modified().unwrap(),
        before_mtime,
        "the ledger was touched"
    );
    assert_eq!(
        listing(dir.path()),
        before_listing,
        "the panel left something behind in the run directory"
    );
    assert_eq!(app.records().len(), 2, "and it could still read the ledger");
}

/// Waits for the order just sent to come back, so the next command is not dropped at the in
/// flight guard.
///
/// Asserting it was sent at all is half the point. `dispatch` returns quietly when something
/// is outstanding, and a command that returned quietly is a command a test did not exercise.
fn settle(app: &mut App, what: &str) {
    assert!(app.in_flight.is_some(), "{what} never reached the worker");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.in_flight.is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "{what} was sent and never came back"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        app.tick();
    }
}

/// The bug as a person meets it. One click on PING took the in flight slot, and since a ping's
/// whole reply is a heartbeat and a heartbeat settled nothing, it was never given back. From
/// that click on, every command button in the panel did nothing until a restart.
#[test]
fn a_ping_does_not_wedge_every_command_that_follows_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());

    app.ping();
    settle(&mut app, "ping");

    app.stop(id("brief"));
    assert!(
        app.in_flight.is_some(),
        "a stop after a ping was dropped at the in flight guard"
    );
    settle(&mut app, "stop");

    app.stop_all();
    assert!(app.in_flight.is_some(), "and so was a stop all");
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Not knowing is a state of its own, and it is where the panel starts.
#[test]
fn the_link_starts_unknown_and_becomes_silent_once_asked() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    assert_eq!(app.link, Link::Unknown);
    assert!(!app.link.can_command());

    app.apply(Outcome::Heartbeat(Link::Silent {
        why: "no daemon answering on run/aosd.sock".into(),
    }));
    assert_eq!(app.link.word(), "NO DAEMON");
    assert!(!app.link.can_command());
}

/// The worker's own ping must not clear something a person is waiting on.
#[test]
fn a_heartbeat_does_not_settle_an_outstanding_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    app.stop(id("brief"));
    assert!(app.in_flight.is_some());

    app.apply(Outcome::Heartbeat(Link::Unknown));
    assert!(app.in_flight.is_some(), "the stop is still out");

    app.apply(Outcome::Note {
        text: "stopped brief, exit 0".into(),
        tone: Tone::Good,
    });
    assert!(app.in_flight.is_none());
    assert_eq!(app.notes[0].text, "stopped brief, exit 0");
}

/// One order at a time. A second click while a stop is out would send a second stop.
#[test]
fn a_second_order_is_not_sent_while_one_is_out() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    app.stop(id("brief"));
    let first = app.in_flight.clone();
    app.stop_all();
    assert_eq!(app.in_flight, first, "the stop all was not sent");
}

/// Harness invariant. There is no path from a loaded spec to a running agent that does not go
/// through a plan the daemon offered.
#[test]
fn nothing_can_be_committed_without_a_plan_the_daemon_offered() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());

    // From nothing.
    app.commit();
    assert!(
        app.in_flight.is_none(),
        "a commit was sent from an idle panel"
    );

    // From a spec that has only been read off disk.
    app.apply(Outcome::SpecLoaded {
        spec: spec("risky", RiskTier::Destructive),
        verdict: Some(aos_core::Verdict::Prompt),
    });
    app.commit();
    assert!(app.in_flight.is_none(), "a commit was sent with no plan");

    // Asking is a start with no commit quoted, which runs nothing.
    app.request_plan();
    match app.in_flight.clone() {
        Some(Order::Plan(sent)) => assert_eq!(sent.id, id("risky")),
        other => panic!("expected a planning call, got {other:?}"),
    }
    app.in_flight = None;

    // Now the daemon offers one, and only now is there something to commit.
    app.apply(Outcome::PlanOffered {
        plan: PlanId::quoted("4f2a"),
        agent: id("risky"),
        tier: RiskTier::Destructive,
        summary: "risky would run /usr/bin/echo [\"hello\"] at tier destructive".into(),
    });
    app.commit();
    match app.in_flight.clone() {
        Some(Order::Commit { spec, plan }) => {
            assert_eq!(plan, PlanId::quoted("4f2a"));
            assert_eq!(
                spec.id,
                id("risky"),
                "the plan's own spec is what is committed"
            );
        }
        other => panic!("expected a commit, got {other:?}"),
    }
}

/// A plan for a different agent must not become committable. Planning something harmless and
/// committing something else is the exact thing the handshake exists to stop.
#[test]
fn a_plan_for_another_agent_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    app.apply(Outcome::SpecLoaded {
        spec: spec("harmless", RiskTier::Write),
        verdict: None,
    });
    app.request_plan();
    app.in_flight = None;

    app.apply(Outcome::PlanOffered {
        plan: PlanId::quoted("dead"),
        agent: id("something-else"),
        tier: RiskTier::Destructive,
        summary: "something-else would run /bin/rm".into(),
    });

    assert_eq!(app.start, StartFlow::Idle);
    app.commit();
    assert!(app.in_flight.is_none());
    assert!(app.notes[0].text.contains("dropped"), "{:?}", app.notes[0]);
}

/// An unasked for plan is not something to offer somebody a button for.
#[test]
fn a_plan_nobody_asked_for_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    app.apply(Outcome::PlanOffered {
        plan: PlanId::quoted("dead"),
        agent: id("risky"),
        tier: RiskTier::Destructive,
        summary: "risky would run /bin/rm".into(),
    });
    assert_eq!(app.start, StartFlow::Idle);
    assert!(app.notes[0].text.contains("never asked for"));
}

/// A refused commit goes back to the beginning. Leaving the button armed on a plan the daemon
/// has already rejected offers a retry that cannot work.
#[test]
fn a_refused_commit_disarms_the_button() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    app.start = StartFlow::Offered {
        spec: spec("risky", RiskTier::Destructive),
        plan: PlanId::quoted("4f2a"),
        agent: id("risky"),
        tier: RiskTier::Destructive,
        summary: "risky would run /usr/bin/echo".into(),
    };
    app.commit();
    app.apply(Outcome::Note {
        text: "refused: no such plan".into(),
        tone: Tone::Bad,
    });
    assert_eq!(app.start, StartFlow::Idle);
}

/// The daemon may allow a tier outright, in which case the planning call starts it and no plan
/// comes back. The panel must not sit there looking like it is still waiting for one.
#[test]
fn a_start_the_policy_allowed_outright_ends_the_flow() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    app.apply(Outcome::SpecLoaded {
        spec: spec("reader", RiskTier::Read),
        verdict: Some(aos_core::Verdict::Allow),
    });
    app.request_plan();
    app.apply(Outcome::Note {
        text: "the policy allowed this outright, so it is already running as pid 41".into(),
        tone: Tone::Good,
    });
    assert_eq!(app.start, StartFlow::Idle);
    assert!(app.notes[0].text.contains("already running"));
}

#[test]
fn only_the_most_recent_notes_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    for i in 0..(NOTES_KEPT + 4) {
        app.note(&format!("note {i}"), Tone::Good);
    }
    assert_eq!(app.notes.len(), NOTES_KEPT);
    assert_eq!(app.notes[0].text, format!("note {}", NOTES_KEPT + 3));
}

/// Naming no spec file must complain rather than send a start for nothing.
#[test]
fn loading_with_no_path_named_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    app.load_spec();
    assert!(app.in_flight.is_none());
    assert!(app.notes[0].text.contains("no spec file"));
}
