//! Interpret a completed repair report without inventing missing state.
use super::diagnostic;
use crate::providers::health::{Diagnostic, Health, Metric, Reading};
use serde_json::Value;

pub(super) fn read(rows: &[Value]) -> Option<Diagnostic> {
    if rows.iter().any(|row| {
        !row["status"].as_str().is_some_and(known_status)
            || !row["repo"].as_str().is_some_and(|repo| !repo.is_empty())
    }) {
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
            r["status"].as_str().is_some_and(|s| {
                [
                    "Prepared.",
                    "Submitted for review.",
                    "Already submitted for review.",
                ]
                .iter()
                .any(|prefix| s.starts_with(prefix))
            })
        })
        .collect();
    if let Some(row) = blocked {
        let mut found = diagnostic(
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
        );
        if let Some(prepared) = ready.first() {
            found.summary.push_str(&format!(
                " {} repair(s) ready for review: {}",
                ready.len(),
                subject(prepared)
            ));
            found = actions(found, prepared);
        }
        return Some(found);
    }
    if let Some(row) = ready.first() {
        let found = diagnostic(
            "review",
            Health::Healthy,
            format!(
                "{} repair(s) ready for review: {}",
                ready.len(),
                subject(row)
            ),
        );
        return Some(actions(found, row));
    }
    Some(diagnostic(
        "idle",
        Health::Healthy,
        "Evan has no local repairs waiting for review",
    ))
}

fn actions(mut found: Diagnostic, row: &Value) -> Diagnostic {
    if let Some(command) = review_command(row) {
        found = found.with(Metric::new("review_command", Reading::Text(command), ""));
        if let (Some(repo), Some(url)) = (row["repo"].as_str(), row["pr"].as_str()) {
            let prefix = format!("https://github.com/{repo}/pull/");
            if url.strip_prefix(&prefix).is_some_and(|number| {
                number.bytes().all(|c| c.is_ascii_digit())
                    && number.parse::<u64>().is_ok_and(|n| n > 0)
            }) {
                found = found.with(Metric::new(
                    "pull_request_url",
                    Reading::Text(url.into()),
                    "",
                ));
            }
        }
    }
    found
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

fn known_status(status: &str) -> bool {
    [
        "Blocked.",
        "Queued.",
        "Waiting for ",
        "Prepared.",
        "Already submitted for review.",
        "Submitted for review.",
        "Idle.",
    ]
    .iter()
    .any(|prefix| status.starts_with(prefix))
}
