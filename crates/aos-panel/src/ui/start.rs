//! Plan then commit, drawn as the three states it actually has.
//!
//! Read a spec off disk, ask the daemon what starting it would take, and only if a plan comes
//! back is there anything to commit. There is no fourth state and no shortcut between them.
//! The button that commits appears in one place, and it exists only while the panel holds a
//! plan the daemon offered for that exact spec.

use eframe::egui::{Button, RichText, Ui};

use aos_core::{AgentSpec, Verdict};

use crate::app::{App, StartFlow};
use crate::theme;

use super::widgets;

/// The plan then commit handshake, drawn as the three states it actually has.
pub fn draw(app: &mut App, ui: &mut Ui, ready: bool) {
    ui.horizontal(|ui| {
        widgets::small(ui, "START");
        ui.add(
            eframe::egui::TextEdit::singleline(&mut app.spec_path)
                .hint_text("path to an agent spec, the same file aos start takes")
                .font(theme::body())
                .desired_width(360.0),
        );
        let read = Button::new(RichText::new("READ SPEC").font(theme::label()));
        // Reading a file starts nothing, so this one does not need a daemon.
        if ui.add_enabled(app.in_flight.is_none(), read).clicked() {
            app.load_spec();
        }
    });

    match app.start.clone() {
        StartFlow::Idle => {}
        StartFlow::Loaded { spec, verdict } => loaded(app, ui, ready, &spec, verdict),
        StartFlow::Planning { spec } => {
            ui.label(
                RichText::new(format!(
                    "asking the daemon what starting {} would take. Nothing has been \
                     committed",
                    spec.id
                ))
                .font(theme::label())
                .color(theme::ACCENT),
            );
        }
        StartFlow::Offered {
            plan,
            agent,
            tier,
            summary,
            ..
        } => offered(
            app,
            ui,
            ready,
            &plan.to_string(),
            &agent.to_string(),
            tier,
            &summary,
        ),
    }
}

fn loaded(app: &mut App, ui: &mut Ui, ready: bool, spec: &AgentSpec, verdict: Option<Verdict>) {
    ui.horizontal(|ui| {
        widgets::chip(ui, spec.id.as_str(), theme::TEXT);
        widgets::dim(ui, &format!("{} {:?}", spec.program, spec.args));
        widgets::chip(ui, &format!("CEILING {}", spec.ceiling), theme::COLD);
    });

    // What the policy file in the run directory says. The daemon has the last word and may
    // hold an older policy, so this labels the button and is never relied on.
    let (word, colour, enabled) = match verdict {
        Some(Verdict::Prompt) => (
            "ASK FOR A PLAN, this runs nothing".to_string(),
            theme::ACCENT,
            true,
        ),
        Some(Verdict::Allow) => (
            format!(
                "START IT NOW, the policy allows tier {} outright so no plan is offered",
                spec.ceiling
            ),
            theme::WARN,
            true,
        ),
        Some(Verdict::Deny) => (
            "the policy denies this, so it would only be refused".to_string(),
            theme::BAD,
            false,
        ),
        None => (
            "ASK THE DAEMON, no readable policy here so what happens next is not known".to_string(),
            theme::UNKNOWN,
            true,
        ),
    };

    ui.horizontal(|ui| {
        let ask = Button::new(RichText::new(word).font(theme::body()).color(colour));
        if ui.add_enabled(ready && enabled, ask).clicked() {
            app.request_plan();
        }
        if ui
            .add_enabled(
                app.in_flight.is_none(),
                Button::new(RichText::new("FORGET IT").font(theme::label())),
            )
            .clicked()
        {
            app.abandon_start();
        }
    });
}

/// The plan the daemon offered, and the second click.
///
/// The summary is the daemon's own sentence, shown whole. The panel does not paraphrase it,
/// because the thing being agreed to has to be the thing that was described.
fn offered(
    app: &mut App,
    ui: &mut Ui,
    ready: bool,
    plan: &str,
    agent: &str,
    tier: aos_core::RiskTier,
    summary: &str,
) {
    eframe::egui::Frame::none()
        .fill(theme::RAISED)
        .stroke(theme::edge(theme::ACCENT))
        .inner_margin(eframe::egui::Margin::same(8.0))
        .rounding(theme::CORNER)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::chip(ui, "PLAN OFFERED", theme::ACCENT);
                widgets::chip(ui, &format!("TIER {tier}"), theme::COLD);
                widgets::dim(ui, &format!("plan {plan} for {agent}"));
            });
            ui.label(
                RichText::new(summary)
                    .font(theme::body())
                    .color(theme::TEXT),
            );
            ui.label(
                RichText::new("nothing has run. Committing is what makes it happen")
                    .font(theme::label())
                    .color(theme::FAINT),
            );
            ui.horizontal(|ui| {
                let commit = Button::new(
                    RichText::new("COMMIT THIS PLAN")
                        .font(theme::body())
                        .color(theme::ACCENT),
                );
                if ui.add_enabled(ready, commit).clicked() {
                    app.commit();
                }
                if ui
                    .add_enabled(
                        app.in_flight.is_none(),
                        Button::new(RichText::new("DISCARD").font(theme::label())),
                    )
                    .clicked()
                {
                    app.abandon_start();
                }
            });
        });
}
