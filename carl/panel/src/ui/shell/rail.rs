//! The persistent left rail: what is true no matter which screen you are on.

use eframe::egui::{Align, Layout, Rect, RichText, SidePanel, Ui, pos2, vec2};

use crate::app::{App, Tab};
use crate::theme;
use crate::ui::vitals::{self, Vitals};
use crate::ui::widgets;

/// Navigation only. Detailed inventories belong in their own tabs.
pub const WIDTH: f32 = 184.0;

pub fn draw(app: &mut App, ctx: &eframe::egui::Context) {
    let v = vitals::read(&app.snapshot);
    SidePanel::left("rail")
        .exact_width(WIDTH)
        .resizable(false)
        .frame(
            eframe::egui::Frame::none()
                .fill(theme::PANEL)
                .stroke(theme::hairline())
                .inner_margin(eframe::egui::Margin::symmetric(14.0, 16.0)),
        )
        .show(ctx, |ui| {
            // The footer takes its room before the rest gets any, and what is left scrolls.
            // Drawing the content top down and then anchoring the footer bottom up puts them
            // on top of each other the moment the content is tall enough to reach the bottom,
            // which on a short window it is: "nothing has reported" landed on "F9 hide or show".
            // Both regions placed at rectangles taken from the panel itself. Every other
            // arrangement tried here overlapped: a bottom_up footer met the content, a guessed
            // reserve was shorter than the line at this type scale, and a nested panel still
            // let the scroll area paint past it. An explicit rectangle has no opinion to get
            // wrong.
            let whole = ui.max_rect();
            let foot = theme::label().size * 1.6;
            let top = Rect::from_min_max(
                whole.min,
                pos2(whole.max.x, (whole.max.y - foot).max(whole.min.y)),
            );

            ui.allocate_new_ui(
                eframe::egui::UiBuilder::new()
                    .max_rect(top)
                    .layout(Layout::top_down(Align::Min)),
                |ui| {
                    // Narrowed, not replaced. See the note in `card::sized_card`.
                    ui.set_clip_rect(top.intersect(ui.clip_rect()));
                    eframe::egui::ScrollArea::vertical()
                        .id_salt("rail")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            wordmark(ui);
                            ui.add_space(14.0);
                            attachment(app, ui);
                            ui.add_space(16.0);
                            navigation(app, ui, &v);
                        });
                },
            );

            let bottom = Rect::from_min_max(pos2(whole.min.x, top.max.y), whole.max);
            ui.allocate_new_ui(
                eframe::egui::UiBuilder::new()
                    .max_rect(bottom)
                    .layout(Layout::top_down(Align::Min)),
                |ui| {
                    ui.label(
                        RichText::new("Super+F9  show / hide")
                            .font(theme::label())
                            .color(theme::FAINT),
                    );
                },
            );
        });
}

fn wordmark(ui: &mut Ui) {
    ui.label(
        theme::spaced("AOS")
            .font(theme::display())
            .color(theme::TEXT),
    );
    ui.add_space(-4.0);
    ui.label(
        theme::spaced("COMMAND PANEL")
            .font(theme::label())
            .color(theme::FAINT),
    );
}

/// What the panel is attached to, and whether it is live. Never hidden, never abbreviated
/// away. A panel that looks live while disconnected makes every other number on it a lie, and
/// a panel that quietly shows the mock is the same lie with better manners.
fn attachment(app: &App, ui: &mut Ui) {
    widgets::link_badge(ui, &app.link);
    if app.source_name().contains("mock") {
        ui.label(
            RichText::new("mock data")
                .font(theme::label())
                .color(theme::WARN),
        );
    }
}

fn navigation(app: &mut App, ui: &mut Ui, v: &Vitals) {
    for tab in Tab::ALL {
        let selected = app.tab == tab;
        let count = wants_attention(app, tab, v);
        let response = widgets::row(ui, 36.0, selected, false, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    theme::spaced(tab.label())
                        .font(theme::body())
                        .color(if selected { theme::ACCENT } else { theme::TEXT }),
                );
                if count > 0 {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        badge(ui, count);
                    });
                }
            });
        });
        if response.clicked() {
            app.select_tab(tab);
        }
        ui.add_space(3.0);
    }
}

/// A count of things wanting somebody, drawn as a filled token so it reads at a glance.
fn badge(ui: &mut Ui, count: usize) {
    let text = count.to_string();
    let galley = ui
        .painter()
        .layout_no_wrap(text, theme::label(), theme::VOID);
    let (rect, _) = ui.allocate_exact_size(
        vec2(galley.size().x + 14.0, 20.0),
        eframe::egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, theme::CORNER, theme::BAD);
    ui.painter().galley(
        eframe::egui::pos2(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        theme::VOID,
    );
}

/// How many things on a tab want somebody's attention, which is what the rail counts.
pub fn wants_attention(app: &App, tab: Tab, v: &Vitals) -> usize {
    match tab {
        Tab::Overview => v.wants_jj(),
        Tab::Carl => v.decisions,
        Tab::Agents => v.blocked,
        Tab::Diagnostics => app
            .snapshot
            .diagnostics
            .iter()
            .filter(|d| widgets::wants_attention(d.health))
            .count(),
        Tab::Projects => v.projects_blocked,
    }
}
