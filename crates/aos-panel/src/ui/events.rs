//! The ledger itself, one line per record.
//!
//! Columns are painted at fixed offsets rather than laid out, because the whole value of this
//! panel is that a sequence number, a time and an agent name line up down the screen. A
//! horizontal layout would let one long refusal reason push everything below it out of column.
//!
//! Refusals are drawn loudest. A log that only shows what happened cannot answer what
//! something tried to do and was stopped from doing, which is the question anybody actually
//! has when they open this.

use eframe::egui::{Align2, Rect, RichText, ScrollArea, Sense, Ui, vec2};

use aos_core::Record;

use crate::app::App;
use crate::format::{self, Weight};
use crate::theme;

use super::widgets;

/// Where each column starts, in points from the left of the line.
const SEQ: f32 = 8.0;
const TIME: f32 = 66.0;
const AGENT: f32 = 140.0;
const KIND: f32 = 270.0;
const DETAIL: f32 = 372.0;
const LINE: f32 = 17.0;

pub fn draw(app: &mut App, ui: &mut Ui) {
    header(app, ui);

    let now = app.now;
    let newest_first = app.newest_first;
    let refusals_only = app.refusals_only;
    let only = app.selected.clone();

    let shown: Vec<&Record> = app
        .records()
        .iter()
        .filter(|r| !refusals_only || format::is_refusal(r))
        .filter(|r| only.as_ref().is_none_or(|id| &r.agent == id))
        .collect();

    if shown.is_empty() {
        ui.label(
            RichText::new(empty_reason(app))
                .font(theme::label())
                .color(theme::UNKNOWN),
        );
        return;
    }

    ScrollArea::vertical()
        .id_salt("feed")
        // Oldest first means the newest is at the bottom, so the view follows it there.
        .stick_to_bottom(!newest_first)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if newest_first {
                for record in shown.iter().rev() {
                    line(ui, record, now);
                }
            } else {
                for record in &shown {
                    line(ui, record, now);
                }
            }
        });
}

fn empty_reason(app: &App) -> String {
    if app.records().is_empty() {
        return "nothing has been recorded in this run directory".into();
    }
    match (app.refusals_only, &app.selected) {
        (true, Some(agent)) => format!("nothing was refused for {agent}"),
        (true, None) => "nothing has been refused, which is the good outcome".into(),
        (false, Some(agent)) => format!("nothing in the ledger names {agent}"),
        (false, None) => "nothing to show".into(),
    }
}

/// The strip above the feed: what is being shown, and the two things that change it.
fn header(app: &mut App, ui: &mut Ui) {
    widgets::section(ui, "LEDGER");
    ui.horizontal(|ui| {
        let refusals = app.refusals();
        widgets::chip(ui, &format!("{} RECORDS", app.records().len()), theme::DIM);
        widgets::chip(
            ui,
            &format!("{refusals} REFUSED"),
            if refusals > 0 {
                theme::BAD
            } else {
                theme::FAINT
            },
        );
        widgets::chip(ui, &format!("SEQ {}", app.last_seq()), theme::DIM);

        if let Some(agent) = app.selected.clone()
            && ui
                .button(
                    RichText::new(format!("only {agent}, click to clear"))
                        .font(theme::label())
                        .color(theme::ACCENT),
                )
                .clicked()
        {
            app.selected = None;
        }

        ui.checkbox(
            &mut app.refusals_only,
            RichText::new("refusals only").font(theme::label()),
        );
        ui.checkbox(
            &mut app.newest_first,
            RichText::new("newest first").font(theme::label()),
        );
    });
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        widgets::small(ui, "SEQ");
        ui.add_space(18.0);
        widgets::small(ui, "UTC");
        ui.add_space(28.0);
        widgets::small(ui, "AGENT");
        ui.add_space(66.0);
        widgets::small(ui, "EVENT");
        ui.add_space(48.0);
        widgets::small(ui, "DETAIL");
    });
}

fn line(ui: &mut Ui, record: &Record, now: u64) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, LINE), Sense::hover());
    let weight = format::weight(&record.event);
    let color = widgets::weight_color(weight);

    // A refusal gets a band and a bar, so it is findable while scrolling fast and while the
    // screen is being looked at from across a room.
    if weight == Weight::Refusal {
        ui.painter()
            .rect_filled(rect, 0.0, theme::BAD.gamma_multiply(0.12));
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
            0.0,
            theme::BAD,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, theme::RAISED);
    }

    let painter = ui.painter().with_clip_rect(rect);
    let at = |x: f32| rect.left_top() + vec2(x, 2.0);
    let font = theme::label();

    painter.text(
        at(SEQ),
        Align2::LEFT_TOP,
        record.seq.to_string(),
        font.clone(),
        theme::FAINT,
    );
    painter.text(
        at(TIME),
        Align2::LEFT_TOP,
        format::clock(record.at),
        font.clone(),
        theme::FAINT,
    );
    painter.text(
        at(AGENT),
        Align2::LEFT_TOP,
        record.agent.as_str(),
        font.clone(),
        theme::TEXT,
    );
    painter.text(
        at(KIND),
        Align2::LEFT_TOP,
        format::kind(&record.event),
        font.clone(),
        color,
    );
    painter.text(
        at(DETAIL),
        Align2::LEFT_TOP,
        format::detail(&record.event),
        font.clone(),
        if weight == Weight::Refusal {
            theme::TEXT
        } else {
            theme::DIM
        },
    );

    // The age sits on the right, where it can be scanned down without reading the line.
    painter.text(
        rect.right_top() + vec2(-8.0, 2.0),
        Align2::RIGHT_TOP,
        format::ago(now, record.at),
        font,
        theme::FAINT,
    );
}
