//! Problems first, with measurement gaps and the full inventory available on request.
use super::widgets;
use crate::app::App;
use crate::command::WorkspaceRequest;
use crate::model::{Diagnostic, Health, stale};
use crate::theme;
use eframe::egui::{RichText, ScrollArea, Ui};

mod card;
pub mod order;
pub use order::sorted;

fn actionable(d: &Diagnostic, now: u64) -> bool {
    widgets::wants_attention(d.health) || (d.health == Health::Healthy && stale(d, now))
}

pub fn draw(app: &mut App, ui: &mut Ui) {
    ui.checkbox(
        &mut app.diagnostic_details,
        "Show all components and measurements",
    );
    ui.add_space(theme::GAP);
    let mut rows: Vec<_> = app.snapshot.diagnostics.iter().collect();
    rows.sort_by_key(|d| (order::worst_first(d.health), &d.component));
    let attention = rows
        .iter()
        .filter(|d| actionable(d, app.snapshot.at))
        .count();
    let unknown = rows.iter().filter(|d| d.health == Health::Unknown).count();
    widgets::section_count(ui, "NEEDS ATTENTION", attention, theme::DIM);
    let mut investigate = None;
    ScrollArea::vertical()
        .id_salt("diagnostics")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if rows.is_empty() {
                ui.label("No components have reported. Health is unknown.");
            } else if attention == 0 && !app.diagnostic_details {
                ui.label(if unknown > 0 {
                    "No reported faults. Some components could not be measured."
                } else {
                    "No reported faults. All component readings are current."
                });
            }
            if unknown > 0 && !app.diagnostic_details {
                ui.collapsing(format!("Unmeasured components ({unknown} UNKNOWN)"), |ui| {
                    for d in rows.iter().filter(|d| d.health == Health::Unknown) {
                        if brief(ui, d, app.snapshot.at) {
                            investigate = Some(d.component.clone());
                        }
                    }
                });
            }
            for d in &rows {
                let clicked = if app.diagnostic_details {
                    card::draw(ui, d, app.snapshot.at)
                } else if actionable(d, app.snapshot.at) {
                    brief(ui, d, app.snapshot.at)
                } else {
                    false
                };
                if clicked {
                    investigate = Some(d.component.clone());
                }
            }
        });
    if let Some(component) = investigate {
        app.open_workspace(WorkspaceRequest::Investigate { component });
    }
}

fn brief(ui: &mut Ui, d: &Diagnostic, now: u64) -> bool {
    let mut investigate = false;
    widgets::fitted_card(
        ui,
        widgets::Card::default().attention(actionable(d, now)),
        |ui| {
            ui.label(
                RichText::new(&d.component)
                    .font(theme::heading())
                    .color(theme::TEXT),
            );
            ui.horizontal_wrapped(|ui| {
                widgets::state_chip(
                    ui,
                    widgets::health_mark(d.health),
                    widgets::health_label(d.health),
                    widgets::health_color(d.health),
                );
                if let Some(freshness) = widgets::freshness(d, now) {
                    ui.label(
                        RichText::new(freshness)
                            .font(theme::label())
                            .color(theme::DIM),
                    );
                }
            });
            ui.label(
                RichText::new(&d.summary)
                    .font(theme::prose())
                    .color(theme::TEXT),
            );
            investigate = ui.button("Investigate").clicked();
        },
    );
    investigate
}
