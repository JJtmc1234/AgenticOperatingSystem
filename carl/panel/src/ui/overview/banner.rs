//! One health notice, without duplicating the diagnostics inventory.
use crate::app::{App, Tab};
use crate::theme;
use crate::ui::vitals::Vitals;
use eframe::egui::{RichText, Ui};

pub fn draw(app: &mut App, ui: &mut Ui, v: &Vitals) {
    let problems = v.failed + v.degraded + v.held;
    let (text, colour) = if problems > 0 {
        (
            format!("{problems} system issue(s) need attention"),
            theme::WARN,
        )
    } else if v.blocked > 0 {
        (format!("{} assignment(s) blocked", v.blocked), theme::WARN)
    } else if v.working + v.review > 0 {
        (
            format!("{} working, {} ready for review", v.working, v.review),
            theme::TEXT,
        )
    } else {
        ("No active assignments".into(), theme::TEXT)
    };
    ui.label(RichText::new(text).font(theme::heading()).color(colour));
    if problems > 0 && ui.button("View issues").clicked() {
        app.select_tab(Tab::Diagnostics);
    }
    if v.working + v.review + v.blocked == 0 {
        ui.label(
            RichText::new("Agents wait for assigned work. Scheduled workflows run separately.")
                .font(theme::prose())
                .color(theme::DIM),
        );
    }
    if v.unknown > 0 || v.unmeasured > 0 || v.components() == 0 {
        ui.label(
            RichText::new("Some status is unavailable. Open Diagnostics for details.")
                .font(theme::prose())
                .color(theme::UNKNOWN),
        );
    }
}
