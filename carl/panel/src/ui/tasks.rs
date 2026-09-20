//! Recorded assignments, including tasks that were never attached to a project.
use super::widgets;
use crate::app::{App, Tab};
use crate::theme;
use carl::panel::view::{Maybe, TaskView};
use eframe::egui::{RichText, ScrollArea, Ui};

pub fn finished(task: &TaskView) -> bool {
    matches!(task.status.as_str(), "accepted" | "abandoned")
}

pub fn ordered(tasks: &[TaskView]) -> Vec<&TaskView> {
    let mut rows: Vec<_> = tasks.iter().collect();
    rows.sort_by_key(|t| {
        (
            finished(t),
            t.status != "blocked",
            std::cmp::Reverse(t.updated_at),
            &t.id,
        )
    });
    rows
}

pub fn draw(app: &mut App, ui: &mut Ui) {
    let tasks = app.snapshot.tasks.clone();
    let rows = ordered(&tasks);
    let repair_active = super::workflow::active(app).is_some();
    let other_work: Vec<_> = super::overview::roster::active(&app.snapshot.agents)
        .into_iter()
        .filter(|a| !(repair_active && a.name == "evan"))
        .filter(|a| !rows.iter().any(|t| !finished(t) && t.owner == a.name))
        .cloned()
        .collect();
    let active = rows.iter().filter(|t| !finished(t)).count()
        + other_work.len()
        + usize::from(repair_active);
    widgets::section_count(ui, "ACTIVE TASKS", active, theme::DIM);
    if ui.button("Give Carl a task").clicked() {
        app.select_tab(Tab::Carl);
    }
    ScrollArea::vertical().id_salt("tasks").auto_shrink([false, false]).show(ui, |ui| {
        if active == 0 {
            widgets::fitted_card(ui, widgets::Card::default(), |ui| {
                ui.label(RichText::new("No active assignments recorded").font(theme::heading()));
                ui.label("Give Carl a task. Delegated tasks appear here with their owner and recorded status.");
            });
        }
        super::workflow::draw(app, ui);
        for task in rows.iter().filter(|t| !finished(t)) {
            row(app, ui, task);
        }
        for agent in &other_work {
            widgets::fitted_card(ui, widgets::Card::default(), |ui| {
                ui.label(RichText::new(super::agents::work_line(agent).0).font(theme::heading()));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(format!("Owner: {}", agent.name)).clicked() {
                        app.select_agent(&agent.name);
                        app.select_tab(Tab::Agents);
                    }
                    ui.label(agent.status.label());
                });
            });
        }
        let finished: Vec<_> = rows.iter().filter(|t| finished(t)).collect();
        if !finished.is_empty() {
            ui.collapsing(format!("Finished tasks ({})", finished.len()), |ui| {
                for task in finished { row(app, ui, task); }
            });
        }
    });
}

fn row(app: &mut App, ui: &mut Ui, task: &TaskView) {
    widgets::fitted_card(
        ui,
        widgets::Card::default().attention(task.status == "blocked"),
        |ui| {
            ui.label(
                RichText::new(&task.goal)
                    .font(theme::heading())
                    .color(theme::TEXT),
            );
            ui.horizontal_wrapped(|ui| {
                if ui.button(format!("Owner: {}", task.owner)).clicked() {
                    app.select_agent(&task.owner);
                    app.select_tab(Tab::Agents);
                }
                ui.label(
                    RichText::new(&task.status)
                        .font(theme::body())
                        .color(theme::DIM),
                );
                ui.label(format!(
                    "Updated {}",
                    widgets::ago(app.snapshot.at, task.updated_at)
                ));
            });
            ui.push_id(&task.id, |ui| {
                ui.collapsing("Task details", |ui| {
                    ui.label(format!("Assigned by {} · {}", task.assigner, task.id));
                    for requirement in &task.must {
                        ui.label(format!("• {requirement}"));
                    }
                    if let Maybe::Known { value: review } = &task.review {
                        ui.label(format!("Review by {}: {}", review.by, review.why));
                    }
                });
            });
        },
    );
}
