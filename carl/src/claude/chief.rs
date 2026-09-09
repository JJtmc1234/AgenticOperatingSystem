//! The chief can read and delegate. Approval never turns him into an implementer.

use super::permits::{Book, Mode, Surface};
use crate::panel::permission::{Verdict, decision};
use std::path::Path;

/// None means the permitted role still needs JJ's decision in the panel.
pub fn decision_for(home: &Path, surface: &str, call: &serde_json::Value) -> Option<String> {
    let surface = match surface {
        "jj" | "carl" => Surface::Jj,
        "slack" => Surface::Shared,
        _ => return None,
    };
    let tool = call["tool_name"].as_str().unwrap_or("");
    let permit = if tool == "Bash" {
        call["tool_input"]["command"].as_str().and_then(bash_permit)
    } else {
        crate::army::chain::tools_for(crate::army::org::Rank::Chief)
            .into_iter()
            .find(|permitted| permitted == tool)
    };
    let Some(permit) = permit else {
        return Some(decision(
            Verdict::Deny,
            "Carl is the chief. Delegate implementation with carl handoff --from carl --to <lead>. This tool is outside his role.",
        ));
    };
    let book = match Book::load(home) {
        Ok(book) => book,
        Err(error) => return Some(decision(Verdict::Deny, &error.to_string())),
    };
    let configured = book.for_surface(surface);
    let allowed = super::permits::narrow_to_rank(&configured.allow, crate::army::org::Rank::Chief);
    if configured.mode == Mode::BypassPermissions || allowed.contains(&permit) {
        Some(decision(
            Verdict::Allow,
            "Already permitted for this surface and Carl's role",
        ))
    } else {
        None
    }
}

/// Permit one literal command, never shell composition or substitution.
fn bash_permit(command: &str) -> Option<String> {
    let mut quote = None;
    let mut escaped = false;
    for c in command.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote == Some('\'') {
            if c == '\'' {
                quote = None;
            }
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if matches!(c, '$' | '`') {
            return None;
        }
        if quote == Some('"') {
            if c == '"' {
                quote = None;
            }
            continue;
        }
        if matches!(c, '\'' | '"') {
            quote = Some(c);
        } else if ";|&<>\n\r(){}[]*?~#".contains(c) {
            return None;
        }
    }
    let words = shlex::split(command)?;
    if words.first()?.as_str() != "carl" {
        return None;
    }
    let route = words.get(1)?.as_str();
    if route == "handoff" {
        let from: Vec<&str> = words
            .iter()
            .enumerate()
            .filter_map(|(i, word)| {
                if word == "--from" {
                    words.get(i + 1).map(String::as_str)
                } else {
                    word.strip_prefix("--from=")
                }
            })
            .collect();
        if from != ["carl"] {
            return None;
        }
    } else if !matches!(route, "hypr" | "portal") {
        return None;
    }
    Some(format!("Bash(carl {route}:*)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chief_cannot_turn_an_approved_handoff_into_implementation() {
        for command in [
            "python3 -c 'print(1)'",
            "carl handoff --from carl --to adrian hi; touch /tmp/x",
            "carl handoff --from carl --to adrian $(touch /tmp/x)",
            "carl handoff --from olivia --to miles hi",
            "carl handoff --from carl --to adrian hi > /tmp/x",
        ] {
            assert!(bash_permit(command).is_none(), "{command}");
        }
        assert_eq!(
            bash_permit("carl handoff --from carl --to adrian 'Fix the build'"),
            Some(crate::army::chain::HANDOFF.into())
        );
    }

    #[test]
    fn a_preapproved_handoff_does_not_wait_for_another_panel_click() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("permissions.json"),
            r#"{"jj":{"allow":["Bash(carl handoff:*)"]}}"#,
        )
        .unwrap();
        let call = serde_json::json!({"tool_name":"Bash", "tool_input":{
            "command":"carl handoff --from carl --to adrian 'Fix the build'"}});
        let answer = decision_for(home.path(), "jj", &call).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&answer).unwrap()["hookSpecificOutput"]["permissionDecision"],
            "allow"
        );
        assert!(decision_for(home.path(), "slack", &call).is_none());
    }

    #[test]
    fn implementation_is_denied_even_when_the_surface_bypasses_prompts() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("permissions.json"),
            r#"{"jj":{"mode":"bypassPermissions"}}"#,
        )
        .unwrap();
        let answer =
            decision_for(home.path(), "jj", &serde_json::json!({"tool_name":"Write"})).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&answer).unwrap()["hookSpecificOutput"]["permissionDecision"],
            "deny"
        );
    }
}
