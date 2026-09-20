//! Show problems first. Measurements and healthy components are an explicit opt-in.
//! Each visible field answers what failed, why, whether the reading is current, or how to act.

use super::widgets;
use crate::app::App;
use crate::command::WorkspaceRequest;
use crate::model::{Health, stale};
use crate::theme;
use eframe::egui::{RichText, ScrollArea, Ui};

mod card;
pub mod order;
pub use order::sorted;

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
        .filter(|d| d.health != Health::Healthy || stale(d, app.snapshot.at))
        .count();
    widgets::section_count(ui, "NEEDS ATTENTION", attention, theme::DIM);
    let mut investigate = None;
    ScrollArea::vertical()
        .id_salt("diagnostics")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if rows.is_empty() {
                ui.label("No components have reported. Health is unknown.");
            } else if attention == 0 && !app.diagnostic_details {
                ui.label("No reported problems. All component readings are current.");
            }
            for d in rows {
                if app.diagnostic_details {
                    if card::draw(ui, d, app.snapshot.at) {
                        investigate = Some(d.component.clone());
                    }
                } else if d.health != Health::Healthy || stale(d, app.snapshot.at) {
                    widgets::fitted_card(ui, widgets::Card::default().attention(true), |ui| {
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
                            if let Some(freshness) = widgets::freshness(d, app.snapshot.at) {
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
                        if ui.button("Investigate").clicked() {
                            investigate = Some(d.component.clone());
                        }
                    });
                }
            }
        });
    if let Some(component) = investigate {
        app.open_workspace(WorkspaceRequest::Investigate { component });
    }
}
