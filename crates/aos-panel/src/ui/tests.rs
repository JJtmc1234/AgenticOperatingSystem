//! The drawing, run for real without a window.
//!
//! Unit tests exercise state and never layout, and a panel breaks in the layout: a row that
//! will not fit, a borrow held across a closure, a scroll area nested inside itself. egui is
//! pure computation, so a whole frame can be run here and the text it painted read back. That
//! is the same code path the window uses, minus the window.

use eframe::egui::{Context, RawInput, Rect, Shape, pos2};

use aos_core::{AgentId, Event, ProcessHandle, Record};

use crate::app::App;
use crate::theme;

/// Draws one frame and returns every string that actually reached the screen.
fn painted(app: &mut App) -> Vec<String> {
    let ctx = Context::default();
    theme::install(&ctx);
    let input = RawInput {
        screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(1500.0, 900.0))),
        ..Default::default()
    };
    // Two frames. The first builds the font atlas and lets the panels claim their space, and
    // the second draws into the space they settled on.
    let mut text = Vec::new();
    for _ in 0..2 {
        let output = ctx.run(input.clone(), |ctx| {
            app.tick();
            super::draw(app, ctx);
        });
        text.clear();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut text);
        }
    }
    text
}

fn collect(shape: &Shape, into: &mut Vec<String>) {
    match shape {
        Shape::Text(t) => into.push(t.galley.text().to_string()),
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
        _ => {}
    }
}

fn ledger(dir: &std::path::Path, records: &[Record]) {
    let text: String = records
        .iter()
        .map(|r| format!("{}\n", serde_json::to_string(r).unwrap()))
        .collect();
    std::fs::write(dir.join("events.jsonl"), text).unwrap();
}

fn record(seq: u64, agent: &str, event: Event) -> Record {
    Record {
        seq,
        at: 1_700_000_000 + seq,
        agent: AgentId::new(agent).unwrap(),
        event,
    }
}

/// A whole frame against a realistic ledger, checked by what it put on the glass.
#[test]
fn a_frame_draws_the_agents_the_ledger_and_the_refusal() {
    let dir = tempfile::tempdir().unwrap();
    ledger(
        dir.path(),
        &[
            record(
                1,
                "morning-brief",
                Event::Started {
                    handle: ProcessHandle {
                        pid: 4242,
                        start_token: 9_219_785,
                        // The panel only ever reads a handle, so which boot it came from does not matter here.
                        boot: None,
                    },
                    program: "/usr/bin/python3".into(),
                },
            ),
            record(
                2,
                "tidy-downloads",
                Event::Refused {
                    reason: "/bin/rm is not on the allowlist".into(),
                },
            ),
        ],
    );

    let mut app = App::new(dir.path().to_path_buf());
    let text = painted(&mut app);
    let all = text.join("\n");

    for wanted in [
        "morning-brief",
        "tidy-downloads",
        "RUNNING",
        "REFUSED",
        "pid 4242",
        "/bin/rm is not on the allowlist",
        "STOP EVERY AGENT",
        "2 RECORDS",
        "1 REFUSED",
    ] {
        assert!(all.contains(wanted), "{wanted:?} never reached the screen");
    }
    // The connection has to be on screen whatever it says, and here there is no daemon. The
    // small labels are spread out rather than shrunk, so the drawn text is spaced.
    assert!(
        all.contains(&theme::spaced("NOT ASKED")) || all.contains(&theme::spaced("NO DAEMON")),
        "the connection state was not drawn"
    );
}

/// An empty run directory must draw something that says so rather than an empty frame that
/// looks like a panel which failed to start.
#[test]
fn an_empty_run_dir_draws_the_reason_it_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    let all = painted(&mut app).join("\n");

    assert!(all.contains("no ledger"), "{all}");
    assert!(
        all.contains("there is no ledger in this run directory yet"),
        "{all}"
    );
}

/// The plan the daemon offered has to be readable in full, because the thing being agreed to
/// must be the thing that was described.
#[test]
fn an_offered_plan_is_drawn_with_its_summary_and_a_commit_button() {
    use crate::app::StartFlow;
    use aos_core::{AgentSpec, PlanId, RiskTier};

    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf());
    app.start = StartFlow::Offered {
        spec: Box::new(AgentSpec {
            id: AgentId::new("nightly-backup").unwrap(),
            program: "/usr/bin/rsync".into(),
            args: vec!["-a".into()],
            ceiling: RiskTier::Destructive,
        }),
        plan: PlanId::quoted("7c1f9a2b4d6e8f10"),
        agent: AgentId::new("nightly-backup").unwrap(),
        tier: RiskTier::Destructive,
        summary: "nightly-backup would run /usr/bin/rsync [\"-a\"] at tier destructive".into(),
    };
    let all = painted(&mut app).join("\n");

    assert!(all.contains("PLAN OFFERED"), "{all}");
    assert!(all.contains("7c1f9a2b4d6e8f10"), "the plan id");
    assert!(
        all.contains("nightly-backup would run /usr/bin/rsync"),
        "the daemon's own sentence has to be shown whole"
    );
    assert!(all.contains("COMMIT THIS PLAN"), "the second click");
    assert!(all.contains("nothing has run"), "and what it means");
}
