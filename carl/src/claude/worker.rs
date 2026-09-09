//! Avoid asking twice for the permissions already passed to a named army agent.

use crate::army::{chain, org};
use crate::panel::permission::{Verdict, decision};
use serde_json::Value;

pub fn decision_for(surface: &str, call: &Value) -> Option<String> {
    let agent = org::find(surface)?;
    if !matches!(agent.rank, org::Rank::Lead | org::Rank::Worker) {
        return None;
    }
    let tool = call.get("tool_name")?.as_str()?;
    let permitted = chain::tools_for(agent.rank);
    if permitted.iter().any(|name| name == tool) {
        // No override. Claude still applies its allow list, deny rules and other hooks.
        return Some("{}".into());
    }
    if tool == "ToolSearch" && permitted_lookup(call, &permitted) {
        return Some(decision(
            Verdict::Allow,
            "Load only the named Gmail tools already permitted to this agent",
        ));
    }
    None
}

fn permitted_lookup(call: &Value, permitted: &[String]) -> bool {
    let Some(query) = call["tool_input"]["query"].as_str() else {
        return false;
    };
    let Some(names) = query.strip_prefix("select:") else {
        return false;
    };
    names.split(',').all(|name| {
        let name = name.trim();
        chain::MAIL.contains(&name) && permitted.iter().any(|p| p == name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn existing_rank_permissions_defer_to_the_cli_without_overriding_denies() {
        for who in ["olivia", "miles", "nora"] {
            for tool in chain::tools_for(org::require(who).unwrap().rank) {
                if tool.contains('(') {
                    continue;
                }
                assert_eq!(
                    decision_for(who, &json!({"tool_name":tool})),
                    Some("{}".into())
                );
            }
        }
    }

    #[test]
    fn unknown_surfaces_and_tools_keep_the_approval_path() {
        for (who, tool) in [
            ("unknown", "Read"),
            ("jj", "Read"),
            ("slack", "Read"),
            ("carl", "Write"),
            ("olivia", "Write"),
            ("miles", "unlisted_tool"),
        ] {
            assert!(decision_for(who, &json!({"tool_name":tool})).is_none());
        }
    }

    #[test]
    fn gmail_discovery_is_limited_to_exact_permitted_names() {
        let lookup = |query| json!({"tool_name":"ToolSearch", "tool_input":{"query":query}});
        let allowed = decision_for(
            "miles",
            &lookup(
                "select:mcp__claude_ai_Gmail__search_threads,mcp__claude_ai_Gmail__get_message",
            ),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&allowed).unwrap()["hookSpecificOutput"]["permissionDecision"],
            "allow"
        );
        for query in [
            "gmail",
            "select:",
            "select:Agent",
            "select:mcp__claude_ai_Gmail__delete_message",
            "select:mcp__claude_ai_Gmail__get_message,Agent",
            "select:mcp__claude_ai_Gmail__get_message,",
        ] {
            assert!(decision_for("miles", &lookup(query)).is_none(), "{query}");
        }
    }
}
