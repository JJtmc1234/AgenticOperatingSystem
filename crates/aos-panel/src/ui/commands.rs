//! The half of the panel that can change something.
//!
//! Everything here is disabled unless a daemon has actually answered a ping, because the only
//! thing worse than a button that does nothing is a button that looks like it did something.
//!
//! Start is three deliberate steps and cannot be fewer. Read a spec off disk, ask the daemon
//! what starting it would take, and only if the daemon comes back with a plan is there
//! anything to commit. The commit quotes the plan the daemon offered for that exact spec.

use eframe::egui::{Button, RichText, Slider, Ui};

use crate::app::App;
use crate::format;
use crate::theme;
use crate::worker::{Order, Tone};

use super::widgets;

pub fn draw(app: &mut App, ui: &mut Ui) {
    widgets::section(ui, "COMMANDS");

    let ready = app.link.can_command() && app.in_flight.is_none();

    ui.horizontal(|ui| {
        let kill = Button::new(
            RichText::new("STOP EVERY AGENT")
                .font(theme::body())
                .color(theme::BAD),
        );
        if ui.add_enabled(ready, kill).clicked() {
            app.stop_all();
        }
        ui.add(
            Slider::new(&mut app.grace_secs, 0..=30)
                .text(RichText::new("seconds of grace").font(theme::label())),
        );

        match &app.in_flight {
            Some(order) => ui.label(
                RichText::new(format!("waiting: {}", waiting_for(order)))
                    .font(theme::label())
                    .color(theme::ACCENT),
            ),
            None if !app.link.can_command() => ui.label(
                RichText::new("no daemon has answered, so nothing can be commanded")
                    .font(theme::label())
                    .color(theme::UNKNOWN),
            ),
            None => ui.label(RichText::new("").font(theme::label())),
        };
    });

    ui.add_space(6.0);
    super::start::draw(app, ui, ready);
    ui.add_space(6.0);
    notes(app, ui);
}

/// What the panel has been told since it started. Short on purpose: the ledger is the log, and
/// this is only the handful of answers that never reach it.
fn notes(app: &App, ui: &mut Ui) {
    if app.notes.is_empty() {
        return;
    }
    for note in &app.notes {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format::ago(app.now, note.at))
                    .font(theme::label())
                    .color(theme::FAINT),
            );
            ui.label(
                RichText::new(&note.text)
                    .font(theme::label())
                    .color(match note.tone {
                        Tone::Good => theme::GOOD,
                        Tone::Bad => theme::BAD,
                    }),
            );
        });
    }
}

/// What an outstanding order is called on screen.
pub fn waiting_for(order: &Order) -> String {
    match order {
        Order::Ping => "asking whether a daemon is there".into(),
        Order::Stop { agent, grace_secs } => format!("stopping {agent}, {grace_secs}s of grace"),
        Order::StopAll { grace_secs } => format!("stopping everything, {grace_secs}s of grace"),
        Order::LoadSpec(path) => format!("reading {}", path.display()),
        Order::Plan(spec) => format!("asking what starting {} would take", spec.id),
        Order::Commit { spec, plan } => format!("committing plan {plan} for {}", spec.id),
    }
}
