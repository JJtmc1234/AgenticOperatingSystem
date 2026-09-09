//! Notices need their own wrapping row, independent of the title and clocks.
use crate::{app::App, theme};
use eframe::egui::{Context, Frame, Margin, RichText, TopBottomPanel};

pub(super) fn draw(app: &App, ctx: &Context) {
    let resynced = app.resynced_at.is_some_and(|at| at.elapsed().as_secs() < 6);
    if app.notice.is_none() && !resynced {
        return;
    }
    TopBottomPanel::top("command-notice")
        .frame(
            Frame::none()
                .fill(theme::PANEL)
                .inner_margin(Margin::symmetric(18.0, 6.0)),
        )
        .show(ctx, |ui| {
            if let Some((text, ok)) = &app.notice {
                ui.add(
                    eframe::egui::Label::new(
                        RichText::new(text).font(theme::prose()).color(if *ok {
                            theme::GOOD
                        } else {
                            theme::BAD
                        }),
                    )
                    .wrap(),
                );
            }
            if resynced {
                crate::ui::widgets::state_chip(
                    ui,
                    crate::ui::widgets::Mark::Filled,
                    "RESYNCHRONISED",
                    theme::GOOD,
                );
            }
        });
}
