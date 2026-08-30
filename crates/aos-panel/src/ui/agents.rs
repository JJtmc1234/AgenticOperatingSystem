//! The agents list, folded out of the ledger.
//!
//! Every word here comes from the log. The log is a claim about the machine rather than a
//! measurement of it, so a running agent is drawn as "the log says running" and nothing on
//! this screen pretends to have looked at `/proc`.

use eframe::egui::{Align, Layout, RichText, Ui};

use aos_core::AgentId;

use crate::app::App;
use crate::format;
use crate::rows::AgentRow;
use crate::theme;

use super::widgets;

pub fn draw(app: &mut App, ui: &mut Ui) {
    widgets::section(ui, "AGENTS");

    if app.rows().is_empty() {
        ui.label(
            RichText::new(if app.ledger.exists {
                "the ledger has no records in it yet, so no agent has ever been named"
            } else {
                "there is no ledger in this run directory yet"
            })
            .font(theme::label())
            .color(theme::UNKNOWN),
        );
        return;
    }

    let now = app.now;
    let can_command = app.link.can_command() && app.in_flight.is_none();
    let mut stop: Option<AgentId> = None;
    let mut select: Option<AgentId> = None;

    eframe::egui::ScrollArea::vertical()
        .id_salt("agents")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for agent in app.rows() {
                let selected = app.selected.as_ref() == Some(&agent.id);
                let response = widgets::row(ui, 52.0, selected, |ui| {
                    head(ui, agent, can_command, &mut stop);
                    tail(ui, agent, now);
                });
                if response.clicked() {
                    select = Some(agent.id.clone());
                }
            }
        });

    if let Some(agent) = select {
        app.selected = if app.selected.as_ref() == Some(&agent) {
            None
        } else {
            Some(agent)
        };
    }
    if let Some(agent) = stop {
        app.stop(agent);
    }
}

/// The name, the state, and the one button that acts on this agent.
fn head(ui: &mut Ui, agent: &AgentRow, can_command: bool, stop: &mut Option<AgentId>) {
    let color = widgets::life_color(&agent.life);
    ui.horizontal(|ui| {
        widgets::pip(ui, color, agent.life.is_running());
        ui.label(RichText::new(agent.id.as_str()).font(theme::body()).color(
            if agent.life.is_running() {
                theme::TEXT
            } else {
                theme::DIM
            },
        ));
        widgets::chip(ui, agent.life.word(), color);
        if agent.refusals > 0 {
            widgets::chip(ui, &format!("{} REFUSED", agent.refusals), theme::BAD);
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if agent.life.is_running() {
                let button = eframe::egui::Button::new(
                    RichText::new("STOP")
                        .font(theme::label())
                        .color(theme::TEXT),
                );
                if ui.add_enabled(can_command, button).clicked() {
                    *stop = Some(agent.id.clone());
                }
            }
        });
    });
}

/// The second line: what the state means, the tier, and what happened last.
fn tail(ui: &mut Ui, agent: &AgentRow, now: u64) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(agent.life.detail())
                .font(theme::label())
                .color(theme::FAINT),
        );
        ui.label(RichText::new("|").font(theme::label()).color(theme::RULE));
        // The log carries no ceiling, only the tiers things were actually gated at, so an
        // agent nothing was gated for says the tier is unknown rather than showing the
        // harmless one.
        match agent.highest_tier {
            Some(tier) => ui.label(
                RichText::new(format!("tier {tier} seen"))
                    .font(theme::label())
                    .color(theme::COLD),
            ),
            None => ui.label(
                RichText::new("no tier recorded")
                    .font(theme::label())
                    .color(theme::UNKNOWN),
            ),
        };
        ui.label(RichText::new("|").font(theme::label()).color(theme::RULE));
        ui.label(
            RichText::new(format!(
                "{} {}",
                format::kind(&agent.last.event),
                format::ago(now, agent.last.at)
            ))
            .font(theme::label())
            .color(widgets::weight_color(format::weight(&agent.last.event))),
        );
    });
}
