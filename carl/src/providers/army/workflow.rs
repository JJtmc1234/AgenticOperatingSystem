//! Read Evan's separate repair journal without starting or changing the workflow.
use crate::providers::health::{Diagnostic, Health, Kind, Metric, Reading};
use serde_json::Value;
use std::path::Path;

#[path = "workflow_io.rs"]
mod io;
#[path = "workflow_report.rs"]
mod report;
use io::{bounded, busy};

pub const COMPONENT: &str = "army.workflow.evan";

fn diagnostic(phase: &str, health: Health, summary: impl Into<String>) -> Diagnostic {
    Diagnostic::new(COMPONENT, health, summary, Kind::EventDriven).with(Metric::new(
        "phase",
        Reading::Text(phase.into()),
        "",
    ))
}

pub fn read(home: &Path) -> Diagnostic {
    let root = home.join("evan");
    if !root.join("config.json").exists() {
        return diagnostic(
            "unconfigured",
            Health::Unknown,
            "Evan repairs are not configured",
        );
    }
    let unknown = || {
        diagnostic(
            "unknown",
            Health::Unknown,
            "Evan repair status could not be read",
        )
    };
    let Some(config) =
        bounded(&root.join("config.json")).and_then(|s| serde_json::from_str::<Value>(&s).ok())
    else {
        return unknown();
    };
    let Some(repos) = config.get("repositories").and_then(Value::as_object) else {
        return unknown();
    };
    if repos.is_empty() {
        return diagnostic(
            "unconfigured",
            Health::Unknown,
            "Evan has no repair repositories configured",
        );
    }
    match busy(&root) {
        Some(true) => {
            return diagnostic(
                "working",
                Health::Healthy,
                "Evan is checking or repairing Iris issues",
            );
        }
        None => return unknown(),
        _ => {}
    }
    if !root.join("events.jsonl").exists() {
        return diagnostic(
            "idle",
            Health::Healthy,
            "Evan is configured and waiting for its first run",
        );
    }
    let Some(text) = bounded(&root.join("events.jsonl")) else {
        return unknown();
    };
    let mut last = None;
    for (index, line) in text.lines().enumerate() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            return unknown();
        };
        if event["seq"].as_u64() != Some(index as u64 + 1) {
            return unknown();
        }
        match event["kind"].as_str() {
            Some("run_started" | "run_finished") => last = Some(event),
            Some(_) => {}
            None => return unknown(),
        }
    }
    let Some(event) = last else {
        return unknown();
    };
    if event["kind"] == "run_started" {
        return match busy(&root) {
            Some(true) => diagnostic(
                "working",
                Health::Healthy,
                "Evan is checking or repairing Iris issues",
            ),
            Some(false) => diagnostic(
                "blocked",
                Health::Blocked,
                "Evan's repair run was interrupted. Inspect it before retrying.",
            ),
            None => unknown(),
        };
    }
    let Some(rows) = event["report"]["rows"].as_array() else {
        return unknown();
    };
    report::read(rows).unwrap_or_else(unknown)
}
