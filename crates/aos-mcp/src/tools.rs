//! What the file server can do, and how much damage each of those things could do.
//!
//! One table. A capability that is not in it does not exist, and every entry carries its tier
//! next to its name, so adding a capability without deciding how dangerous it is means not
//! writing the line at all.
//!
//! The tiers are not opinions about how the call is usually used. They are about the worst
//! thing it could do. `write_file` is Write because it can destroy the contents of a file
//! somebody cares about, even though most of the time it creates a new one.
//!
//! Never a raw shell, which is the rule phase 3 was written around. A shell is every tier at
//! once and cannot be described to a policy, so there is no entry for it and no way to add
//! one without noticing.

use aos_core::RiskTier;
use serde_json::{Value, json};

pub struct Tool {
    pub name: &'static str,
    pub tier: RiskTier,
    pub summary: &'static str,
    /// The JSON schema Claude Code is given for the arguments.
    pub schema: fn() -> Value,
}

/// Everything the file server offers.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "list_dir",
        tier: RiskTier::Read,
        summary: "List the entries in a directory, relative to the root.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative to the root. Empty means the root itself." }
                },
                "required": []
            })
        },
    },
    Tool {
        name: "read_file",
        tier: RiskTier::Read,
        summary: "Read a text file, relative to the root.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            })
        },
    },
    Tool {
        name: "find",
        tier: RiskTier::Read,
        summary: "Find files under the root whose name contains a string.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "contains": { "type": "string" },
                    "limit": { "type": "integer", "description": "Defaults to 200." }
                },
                "required": ["contains"]
            })
        },
    },
    Tool {
        name: "write_file",
        tier: RiskTier::Write,
        summary: "Write a text file, creating it or replacing what is there.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "text": { "type": "string" },
                    "plan_id": { "type": "string", "description": "Omit to get a plan back. Quote it to carry the plan out." }
                },
                "required": ["path", "text"]
            })
        },
    },
    Tool {
        name: "make_dir",
        tier: RiskTier::Write,
        summary: "Create a directory, and any missing directories above it.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "plan_id": { "type": "string" }
                },
                "required": ["path"]
            })
        },
    },
    Tool {
        name: "move_file",
        tier: RiskTier::Write,
        summary: "Move or rename a file. Refuses to overwrite anything.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string" },
                    "to": { "type": "string" },
                    "plan_id": { "type": "string" }
                },
                "required": ["from", "to"]
            })
        },
    },
    Tool {
        name: "delete_file",
        tier: RiskTier::Destructive,
        summary: "Delete a file. It does not go to a wastebasket and it does not come back.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "plan_id": { "type": "string" }
                },
                "required": ["path"]
            })
        },
    },
];

pub fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name)
}

/// The catalogue in the shape `tools/list` has to return.
pub fn listing() -> Value {
    let tools: Vec<Value> = TOOLS
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                // The tier is put in front of the agent as well as enforced behind it. An
                // agent that knows a call is Destructive can choose not to make it, and
                // knowing is free.
                "description": format!("{} (risk tier: {})", t.summary, t.tier),
                "inputSchema": (t.schema)(),
            })
        })
        .collect();
    json!({ "tools": tools })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule phase 3 was written around. A shell is every tier at once, so it cannot be
    /// described to a policy, so it does not exist here.
    #[test]
    fn there_is_no_way_to_run_a_command() {
        for t in TOOLS {
            for banned in ["shell", "exec", "run", "command", "bash", "eval", "spawn"] {
                assert!(
                    !t.name.contains(banned),
                    "{} looks like a command runner, which phase 3 does not have",
                    t.name
                );
            }
        }
    }

    /// Anything that can lose data must be above Read, or the policy would let it run without
    /// anybody agreeing to it.
    #[test]
    fn nothing_that_changes_the_disk_is_a_read() {
        for t in TOOLS {
            let changes = matches!(
                t.name,
                "write_file" | "make_dir" | "move_file" | "delete_file"
            );
            assert_eq!(
                changes,
                t.tier > RiskTier::Read,
                "{} is tiered wrongly at {}",
                t.name,
                t.tier
            );
        }
    }

    /// Deleting is the one thing on this list that cannot be undone.
    #[test]
    fn deleting_is_the_only_destructive_one() {
        let destructive: Vec<_> = TOOLS
            .iter()
            .filter(|t| t.tier == RiskTier::Destructive)
            .map(|t| t.name)
            .collect();
        assert_eq!(destructive, vec!["delete_file"]);
    }

    /// Every tool above Read has to accept a plan id, or it could never be committed and the
    /// capability would be dead on arrival.
    #[test]
    fn everything_that_needs_a_plan_can_be_given_one() {
        for t in TOOLS.iter().filter(|t| t.tier > RiskTier::Read) {
            let s = (t.schema)();
            assert!(
                s["properties"].get("plan_id").is_some(),
                "{} cannot be committed",
                t.name
            );
            assert!(
                !s["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("plan_id")),
                "{} must be callable without a plan, to get one",
                t.name
            );
        }
    }

    #[test]
    fn the_listing_names_every_tool_and_its_tier() {
        let l = listing();
        let tools = l["tools"].as_array().unwrap();
        assert_eq!(tools.len(), TOOLS.len());
        for t in tools {
            let d = t["description"].as_str().unwrap();
            assert!(d.contains("risk tier:"), "{d}");
        }
    }

    #[test]
    fn a_tool_that_does_not_exist_is_not_found() {
        assert!(find("read_file").is_some());
        assert!(find("shell").is_none());
        assert!(find("").is_none());
    }
}
