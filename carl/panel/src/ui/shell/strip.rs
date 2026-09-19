//! Page title. Technical clocks are available on hover rather than repeated on every screen.
use crate::app::App;
use crate::theme;
use eframe::egui::{Id, RichText, TopBottomPanel};
use std::time::Instant;

pub fn draw(app: &mut App, ctx: &eframe::egui::Context) {
    TopBottomPanel::top("strip")
        .exact_height(52.0)
        .frame(
            eframe::egui::Frame::none()
                .fill(theme::PANEL)
                .inner_margin(eframe::egui::Margin::symmetric(18.0, 12.0)),
        )
        .show(ctx, |ui| {
            let started = ui
                .data_mut(|d| *d.get_temp_mut_or_insert_with(Id::new("panel-began"), Instant::now));
            ui.label(
                RichText::new(app.tab.label())
                    .font(theme::heading())
                    .color(theme::TEXT),
            )
            .on_hover_text(format!(
                "{}\nSequence {}. Panel open {}.",
                app.source_name(),
                app.last_seq(),
                uptime(started.elapsed().as_secs())
            ));
        });
}

pub fn uptime(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        _ => format!("{}h", secs / 3600),
    }
}
