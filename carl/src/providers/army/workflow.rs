//! Read Evan's separate repair journal without starting or changing the workflow.
use crate::providers::health::{Diagnostic, Health, Kind, Metric, Reading};
use serde_json::Value;
use std::path::Path;

#[path = "workflow_io.rs"]
mod io;
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
    for line in text.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            return unknown();
        };
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
    let blocked = rows.iter().find(|r| {
        r["status"]
            .as_str()
            .is_some_and(|s| s.starts_with("Blocked."))
    });
    let ready: Vec<_> = rows
        .iter()
        .filter(|r| {
            r["status"]
                .as_str()
                .is_some_and(|s| s.starts_with("Prepared."))
        })
        .collect();
    if let Some(row) = blocked {
        return diagnostic(
            "blocked",
            Health::Blocked,
            format!("Evan repair blocked: {}", subject(row)),
        );
    }
    if let Some(row) = ready.first() {
        return diagnostic(
            "review",
            Health::Healthy,
            format!(
                "{} repair(s) ready for review: {}",
                ready.len(),
                subject(row)
            ),
        );
    }
    diagnostic(
        "idle",
        Health::Healthy,
        "Evan has no local repairs waiting for review",
    )
}

fn subject(row: &Value) -> String {
    let repo = row["repo"].as_str().unwrap_or("unknown repository");
    let issue = row["issue"]
        .as_u64()
        .map(|n| format!(" #{n}"))
        .unwrap_or_default();
    format!("{repo}{issue}").chars().take(240).collect()
}
