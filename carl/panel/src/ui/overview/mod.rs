//! The default view shows decisions and current work. Inventory stays on its own screens.
use crate::app::{App, Tab};
use crate::theme;
use crate::ui::vitals;
use eframe::egui::{RichText, Ui};

mod activity;
mod attention;
mod banner;
mod feed;
pub(crate) mod roster;
pub mod work;
pub use work::ordered as project_order;

#[cfg(test)]
mod tests;

pub fn draw(app: &mut App, ui: &mut Ui) {
    let v = vitals::read(&app.snapshot);
    eframe::egui::ScrollArea::vertical()
        .id_salt("overview")
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(920.0));
            banner::draw(app, ui, &v);
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new("Give Carl a task").font(theme::body()))
                    .clicked()
                {
                    app.select_tab(Tab::Carl);
                }
                if ui.button("Talk to Evan").clicked() {
                    app.select_agent("evan");
                    app.select_tab(Tab::Agents);
                }
            });
            ui.add_space(22.0);
            if vitals::needs(&app.snapshot)
                .iter()
                .any(|n| n.goes_to != Tab::Diagnostics)
            {
                attention::draw(app, ui, &v);
                ui.add_space(18.0);
            }
            roster::draw(app, ui);
            ui.add_space(22.0);
            ui.collapsing("Recent events", |ui| {
                feed::draw(app, ui);
            });
        });
}
