//! A repair can need review without being a conversational task.
use carl::providers::army::workflow::COMPONENT;
use carl::providers::health::Reading;
use eframe::egui::{RichText, Ui};

use crate::ui::widgets;
use crate::{app::App, theme};

pub(crate) fn active(app: &App) -> Option<&crate::model::Diagnostic> {
    let found = app
        .snapshot
        .diagnostics
        .iter()
        .find(|d| d.component == COMPONENT)?;
    let phase = found
        .metrics
        .iter()
        .find_map(|metric| match &metric.value {
            Reading::Text(value) if metric.name == "phase" => Some(value.as_str()),
            _ => None,
        })?;
    (!matches!(phase, "idle" | "unconfigured")).then_some(found)
}

pub(crate) fn draw(app: &App, ui: &mut Ui) -> bool {
    let Some(found) = active(app) else {
        return false;
    };
    let text = |name: &str| {
        found.metrics.iter().find_map(|metric| match &metric.value {
            Reading::Text(value) if metric.name == name => Some(value.as_str()),
            _ => None,
        })
    };
    widgets::fitted_card(ui, widgets::Card::default(), |ui| {
        ui.label(
            RichText::new("EVAN REPAIR WORKFLOW")
                .font(theme::label())
                .color(theme::COLD),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(&found.summary)
                .font(theme::prose())
                .color(theme::TEXT),
        );
        if let Some(url) = text("pull_request_url") {
            ui.add_space(8.0);
            ui.hyperlink_to("Open draft PR", url);
        } else if let Some(command) = text("review_command") {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Inspect the patch, test results and independent review.")
                    .font(theme::prose())
                    .color(theme::DIM),
            );
            if ui.button("Copy review command").clicked() {
                ui.output_mut(|out| out.copied_text = command.to_owned());
            }
            ui.label(
                RichText::new("Paste into a terminal. Reviewing does not publish the repair.")
                    .font(theme::prose())
                    .color(theme::DIM),
            );
        }
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::Tab, model::AgentStatus, source::MockPanelDataSource, ui::probe};
    use carl::providers::health::{Diagnostic, Health, Kind, Metric};

    #[test]
    fn a_prepared_repair_has_an_action_instead_of_the_empty_task_card() {
        for size in [probe::BIG, probe::SMALL] {
            let mut app = App::new(Box::new(MockPanelDataSource::new()));
            let mut completed = app.snapshot.tasks[0].clone();
            completed.owner = "evan".into();
            completed.status = "accepted".into();
            app.snapshot.tasks = vec![completed];
            let evan = app
                .snapshot
                .agents
                .iter_mut()
                .find(|a| a.name == "evan")
                .unwrap();
            evan.task = None;
            evan.status = AgentStatus::AwaitingReview;
            app.agent = Some("evan".into());
            app.snapshot.diagnostics.push(
                Diagnostic::new(
                    COMPONENT,
                    Health::Healthy,
                    "1 repair ready for review: JJtmc1234/Holoprojector #2",
                    Kind::EventDriven,
                )
                .with(Metric::new("phase", Reading::Text("review".into()), ""))
                .with(Metric::new(
                    "review_command",
                    Reading::Text(
                        "carl evan review --repo JJtmc1234/Holoprojector --issue 2".into(),
                    ),
                    "",
                )),
            );
            let frame = probe::tab(&mut app, Tab::Agents, size);
            assert!(frame.says("REPAIR WORKFLOW"));
            assert!(frame.says("Copy review command"));
            assert!(!frame.says("NO TASK IN HAND"));
            assert!(
                frame.collisions().is_empty(),
                "{}",
                probe::describe_pairs(&frame.collisions())
            );
            assert!(
                frame.cut_off().is_empty(),
                "{}",
                probe::describe(&frame.cut_off())
            );
            app.snapshot
                .diagnostics
                .last_mut()
                .unwrap()
                .metrics
                .push(Metric::new(
                    "pull_request_url",
                    Reading::Text("https://github.com/JJtmc1234/Holoprojector/pull/3".into()),
                    "",
                ));
            let frame = probe::tab(&mut app, Tab::Agents, size);
            assert!(frame.says("Open draft PR"));
            assert!(!frame.says("Copy review command"));
            assert!(
                frame.collisions().is_empty(),
                "{}",
                probe::describe_pairs(&frame.collisions())
            );
            assert!(
                frame.cut_off().is_empty(),
                "{}",
                probe::describe(&frame.cut_off())
            );
        }
    }
}
