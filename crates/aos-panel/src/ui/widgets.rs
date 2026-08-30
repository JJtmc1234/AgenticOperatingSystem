//! The few pieces the whole interface is built from.
//!
//! Shared so the density stays consistent. A panel drifts into noise when every part invents
//! its own way of showing a state, so there is one pip, one chip, one section head and one
//! row, and everything uses them.

use eframe::egui::{Align, Color32, Layout, Rect, Response, RichText, Sense, Ui, Vec2, vec2};

use crate::format::Weight;
use crate::link::Link;
use crate::rows::Life;
use crate::theme;

/// The colour a feed line is drawn in. Refusals are the only thing in the feed that is red.
pub const fn weight_color(weight: Weight) -> Color32 {
    match weight {
        Weight::Refusal => theme::BAD,
        Weight::Notable => theme::WARN,
        Weight::Ordinary => theme::DIM,
        Weight::Unknown => theme::UNKNOWN,
    }
}

/// The colour of an agent's state. Running is the accent because it is the thing on this
/// screen that is happening now.
pub const fn life_color(life: &Life) -> Color32 {
    match life {
        Life::Running { .. } => theme::ACCENT,
        Life::Ended {
            how: crate::rows::Ending::Refused,
            ..
        } => theme::BAD,
        Life::Ended {
            how: crate::rows::Ending::Lost,
            ..
        } => theme::WARN,
        Life::Ended { code: Some(0), .. } => theme::GOOD,
        Life::Ended { code: Some(_), .. } => theme::WARN,
        // No exit code at all. A gap, so it is drawn as one.
        Life::Ended { code: None, .. } => theme::UNKNOWN,
        Life::NeverRan => theme::UNKNOWN,
    }
}

pub const fn link_color(link: &Link) -> Color32 {
    match link {
        Link::Answering { .. } => theme::GOOD,
        Link::Silent { .. } => theme::BAD,
        Link::Unknown => theme::UNKNOWN,
    }
}

/// A small square carrying a state colour. Hollow when nothing is known, because an empty
/// outline reads as absence where a filled grey dot reads as a state somebody chose.
pub fn pip(ui: &mut Ui, color: Color32, filled: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
    if filled {
        ui.painter().rect_filled(rect, 1.0, color);
    } else {
        ui.painter().rect_stroke(rect, 1.0, theme::edge(color));
    }
}

/// A short state word in its own colour.
pub fn chip(ui: &mut Ui, text: &str, color: Color32) {
    ui.label(
        RichText::new(text)
            .font(theme::label())
            .color(color)
            .background_color(theme::RAISED),
    );
}

/// A dim all caps label, spread rather than shrunk.
pub fn small(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(theme::spaced(text))
            .font(theme::label())
            .color(theme::FAINT),
    );
}

pub fn dim(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::label()).color(theme::DIM));
}

/// A section heading with a hairline running off to the right of it, which is what gives the
/// layout structure without drawing a box around everything.
pub fn section(ui: &mut Ui, title: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(theme::spaced(title))
                .font(theme::label())
                .color(theme::DIM),
        );
        let y = ui.cursor().center().y;
        let x0 = ui.cursor().left() + 6.0;
        let x1 = ui.max_rect().right();
        if x1 > x0 {
            ui.painter().hline(x0..=x1, y, theme::hairline());
        }
    });
    ui.add_space(4.0);
}

/// A row that can be picked, drawn as a band rather than as a button. Selection is a bar in
/// the accent down the left plus a lifted background, because a screen of bordered rows is a
/// screen of boxes.
pub fn row(ui: &mut Ui, height: f32, selected: bool, add: impl FnOnce(&mut Ui)) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());

    let fill = if selected {
        theme::HOVER
    } else if response.hovered() {
        theme::RAISED
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, theme::CORNER, fill);
    }
    if selected {
        let bar = Rect::from_min_size(rect.min, vec2(2.0, rect.height()));
        ui.painter().rect_filled(bar, 0.0, theme::ACCENT);
    }

    let inner = rect.shrink2(vec2(10.0, 3.0));
    let mut child = ui.new_child(
        eframe::egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    add(&mut child);
    response
}

/// A key and its value on one line. `None` draws the words for a gap rather than an empty
/// space, so a missing figure is visibly missing rather than looking like a drawing fault.
pub fn field(ui: &mut Ui, key: &str, value: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).font(theme::label()).color(theme::FAINT));
        match value {
            Some(v) => ui.label(RichText::new(v).font(theme::body()).color(theme::TEXT)),
            None => ui.label(
                RichText::new("not known")
                    .font(theme::label())
                    .color(theme::UNKNOWN),
            ),
        };
    });
}

/// The connection indicator, which is on screen at all times.
///
/// The one thing here allowed to be alarming, because a panel that looks live while nothing is
/// answering makes every button on it a lie.
pub fn link_badge(ui: &mut Ui, link: &Link) {
    let color = link_color(link);
    pip(ui, color, !matches!(link, Link::Unknown));
    ui.label(
        RichText::new(theme::spaced(link.word()))
            .font(theme::label())
            .color(color),
    );
}

/// The sentence under the badge. Its own line, because the client names the fix in it and that
/// sentence is longer than anything else on the strip.
pub fn link_sentence(ui: &mut Ui, link: &Link) {
    ui.label(RichText::new(link.sentence()).font(theme::label()).color(
        if matches!(link, Link::Silent { .. }) {
            theme::DIM
        } else {
            theme::FAINT
        },
    ));
}
