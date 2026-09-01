//! The frame everything else sits in.
//!
//! A status strip along the top, the agents down the left, the ledger filling the middle, and
//! the commands along the bottom. Four regions, fixed, because this panel is looked at rather
//! than navigated and a menu would only hide one of them.
//!
//! The strip is the important part. It carries the two freshnesses that matter and keeps them
//! apart: whether a **daemon** is answering, and when the **ledger** was last read. They fail
//! separately, and a panel that showed one number for both would call the whole screen stale
//! every time the daemon went down, when in fact the history is still exactly true.

use eframe::egui::{
    Align, CentralPanel, Context, Frame, Layout, Margin, RichText, SidePanel, TopBottomPanel,
};

use crate::app::App;
use crate::format;
use crate::theme;

use super::widgets;

pub fn draw(app: &mut App, ctx: &Context) {
    strip(app, ctx);
    TopBottomPanel::bottom("commands")
        .resizable(true)
        .default_height(210.0)
        .frame(panel_frame())
        .show(ctx, |ui| super::commands::draw(app, ui));
    SidePanel::left("agents")
        .resizable(true)
        .default_width(560.0)
        .frame(panel_frame())
        .show(ctx, |ui| super::agents::draw(app, ui));
    CentralPanel::default()
        .frame(panel_frame())
        .show(ctx, |ui| super::events::draw(app, ui));
}

fn panel_frame() -> Frame {
    Frame::none()
        .fill(theme::VOID)
        .inner_margin(Margin::symmetric(14.0, 10.0))
}

fn strip(app: &mut App, ctx: &Context) {
    TopBottomPanel::top("strip")
        .exact_height(88.0)
        .frame(
            Frame::none()
                .fill(theme::PANEL)
                .inner_margin(Margin::symmetric(14.0, 8.0)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(theme::spaced("AOS"))
                        .font(theme::big())
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(theme::spaced("COMMAND PANEL"))
                        .font(theme::label())
                        .color(theme::FAINT),
                );
                ui.add_space(16.0);
                widgets::link_badge(ui, &app.link);

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            app.in_flight.is_none(),
                            eframe::egui::Button::new(RichText::new("PING").font(theme::label())),
                        )
                        .clicked()
                    {
                        app.ping();
                    }
                    ui.label(
                        RichText::new(app.run_dir().display().to_string())
                            .font(theme::label())
                            .color(theme::FAINT),
                    );
                });
            });

            // The badge above says the state in one word. This says why, and on a machine with
            // no daemon running it is the line that names the fix. It was written and never
            // drawn, so the strip showed NO DAEMON and withheld what to do about it. See bug 32.
            ui.horizontal(|ui| widgets::link_sentence(ui, &app.link));

            ui.horizontal(|ui| {
                ui.label(RichText::new(ledger_line(app)).font(theme::label()).color(
                    if app.ledger.error.is_some() {
                        theme::BAD
                    } else {
                        theme::DIM
                    },
                ));
                if let Some(warning) = disagreement(app) {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(warning)
                            .font(theme::label())
                            .color(theme::WARN),
                    );
                }
            });
        });
}

/// What is known about the ledger, including when it was last read.
///
/// The read time is on screen because the ledger is polled rather than pushed. A record that
/// was appended a moment ago is not on this screen yet, and a person deciding whether to act
/// on what they can see needs to know how old the answer is.
fn ledger_line(app: &App) -> String {
    if let Some(error) = &app.ledger.error {
        return format!("the ledger could not be read: {error}");
    }
    // An absolute time rather than "0s ago". The age is measured from the same clock reading
    // the frame was drawn with, so a panel that has stopped being drawn would go on claiming
    // it had just read the file. A wall clock time can be checked against a real clock.
    let when = match app.ledger.read_at {
        Some(at) => format!("read at {} UTC", format::clock(at)),
        None => "never read".into(),
    };
    let restarts = match app.ledger.restarts {
        0 => String::new(),
        n => format!(", replaced {n} times while watching"),
    };
    if !app.ledger.exists {
        return format!(
            "no ledger at {} yet, {when}{restarts}",
            app.ledger_path().display()
        );
    }
    format!(
        "ledger {}, {when}, {} records, {} running, {} refused, seq {}{restarts}",
        app.ledger_path().display(),
        app.records().len(),
        app.running(),
        app.refusals(),
        app.last_seq(),
    )
}

/// When the ledger and the daemon do not agree about how many agents there are.
///
/// Not an error and not hidden. The ledger is what was written down and the daemon is what is
/// held right now, and the gap between them is real information: an agent adopted from a
/// previous daemon, or a record still being written, will both show up here.
fn disagreement(app: &App) -> Option<String> {
    let crate::link::Link::Answering { tracking, .. } = &app.link else {
        return None;
    };
    let ledger_says = app.running();
    if *tracking == ledger_says {
        return None;
    }
    Some(format!(
        "the ledger says {ledger_says} running, the daemon says it is supervising {tracking}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::Link;

    /// A panel on a run directory of its own. The directory is handed back so it lives as
    /// long as the test does.
    fn app() -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(dir.path().to_path_buf());
        (app, dir)
    }

    /// A run directory with no ledger says exactly that, rather than showing a zero that looks
    /// like a ledger somebody has read and found empty.
    #[test]
    fn a_missing_ledger_is_named_rather_than_counted_as_zero() {
        let (mut a, _dir) = app();
        a.tick();
        let line = ledger_line(&a);
        assert!(line.contains("no ledger at"), "{line}");
        assert!(line.contains("events.jsonl"), "{line}");
        assert!(line.contains("read at"), "{line}");
    }

    #[test]
    fn a_ledger_that_cannot_be_read_says_so_instead_of_saying_it_is_empty() {
        let (mut a, _dir) = app();
        a.ledger.error = Some("permission denied".into());
        let line = ledger_line(&a);
        assert!(line.contains("could not be read"), "{line}");
    }

    /// Two counts that disagree is information, not a fault to hide.
    #[test]
    fn a_disagreement_between_the_log_and_the_daemon_is_shown() {
        let (mut a, _dir) = app();
        assert_eq!(disagreement(&a), None, "nothing to compare with no daemon");

        a.link = Link::Answering {
            version: "0.1.0".into(),
            tracking: 1,
        };
        let said = disagreement(&a).expect("the ledger says none running, the daemon says one");
        assert!(said.contains("supervising 1"), "{said}");

        a.link = Link::Answering {
            version: "0.1.0".into(),
            tracking: 0,
        };
        assert_eq!(disagreement(&a), None, "they agree");
    }
}
