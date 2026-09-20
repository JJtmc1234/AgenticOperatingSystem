//! Interpret a completed repair report without inventing missing state.
use super::diagnostic;
use crate::providers::health::{Diagnostic, Health, Metric, Reading};
use serde_json::Value;

pub(super) fn read(rows: &[Value]) -> Option<Diagnostic> {
    if rows
        .iter()
        .any(|row| row["status"].as_str().is_none() || row["repo"].as_str().is_none())
    {
        return None;
    }
    let blocked = rows.iter().find(|r| {
        r["status"].as_str().is_some_and(|s| {
            ["Blocked.", "Queued.", "Waiting for "]
                .iter()
                .any(|prefix| s.starts_with(prefix))
        })
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
        return Some(diagnostic(
            "blocked",
            Health::Blocked,
            format!(
                "{}: {}",
                subject(row),
                row["status"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(500)
                    .collect::<String>()
            ),
        ));
    }
    if let Some(row) = ready.first() {
        let mut found = diagnostic(
            "review",
            Health::Healthy,
            format!(
                "{} repair(s) ready for review: {}",
                ready.len(),
                subject(row)
            ),
        );
        if let Some(command) = review_command(row) {
            found = found.with(Metric::new("review_command", Reading::Text(command), ""));
        }
        return Some(found);
    }
    Some(diagnostic(
        "idle",
        Health::Healthy,
        "Evan has no local repairs waiting for review",
    ))
}

fn subject(row: &Value) -> String {
    let repo = row["repo"].as_str().unwrap_or("unknown repository");
    let issue = row["issue"]
        .as_u64()
        .map(|n| format!(" #{n}"))
        .unwrap_or_default();
    format!("{repo}{issue}").chars().take(240).collect()
}

fn review_command(row: &Value) -> Option<String> {
    let repo = row["repo"].as_str()?;
    let parts: Vec<_> = repo.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
                || part.starts_with('-')
                || *part == "."
                || *part == ".."
        })
    {
        return None;
    }
    let issue = row["issue"].as_u64().filter(|n| *n > 0)?;
    Some(format!("carl evan review --repo {repo} --issue {issue}"))
}
