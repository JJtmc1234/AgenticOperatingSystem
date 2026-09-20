use super::*;
use crate::{app::Tab, source::MockPanelDataSource};
use carl::providers::army::workflow::COMPONENT;
use carl::providers::health::{Diagnostic, Health, Kind, Metric, Reading};

#[test]
fn an_evans_assignment_does_not_hide_his_separate_repair_review() {
    for tab_name in [Tab::Tasks, Tab::Agents] {
        let mut app = App::new(Box::new(MockPanelDataSource::new()));
        let mut task = app.snapshot.tasks[0].clone();
        task.owner = "evan".into();
        task.status = "in hand".into();
        task.goal = "Current delegated assignment".into();
        let evan = app
            .snapshot
            .agents
            .iter_mut()
            .find(|agent| agent.name == "evan")
            .unwrap();
        evan.task = Some(task.id.clone());
        app.snapshot.tasks = vec![task];
        app.snapshot.diagnostics.push(
            Diagnostic::new(
                COMPONENT,
                Health::Healthy,
                "Independent repair awaits review",
                Kind::EventDriven,
            )
            .with(Metric::new("phase", Reading::Text("review".into()), ""))
            .with(Metric::new(
                "review_command",
                Reading::Text("carl evan review --repo JJtmc1234/Holoprojector --issue 2".into()),
                "",
            )),
        );
        app.select_agent("evan");
        let frame = tab(&mut app, tab_name, BIG);
        assert!(frame.says("Current delegated assignment"));
        assert!(frame.says("Independent repair awaits review"));
        assert!(frame.says("Copy review command"));
        assert!(
            frame.collisions().is_empty(),
            "{}",
            describe_pairs(&frame.collisions())
        );
        assert!(frame.cut_off().is_empty(), "{}", describe(&frame.cut_off()));
    }
}
