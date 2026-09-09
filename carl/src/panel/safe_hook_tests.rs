#[test]
fn harmless_shell_queries_do_not_need_a_panel_click() {
    let home = tempfile::tempdir().unwrap();
    for surface in ["jj", "carl", "slack", "olivia", "miles"] {
        for command in [
            "pwd",
            "pwd -P",
            "/usr/bin/whoami",
            "uname -a",
            "date -u",
            "uptime",
        ] {
            let call = serde_json::json!({"tool_name":"Bash", "tool_input":{"command":command}});
            let printed = super::run(home.path(), surface, &mut call.to_string().as_bytes());
            let result: serde_json::Value = serde_json::from_str(&printed).unwrap();
            assert_eq!(
                result["hookSpecificOutput"]["permissionDecision"], "allow",
                "{surface}: {command}: {printed}"
            );
        }
    }
}

#[test]
fn directory_and_metadata_queries_do_not_need_a_panel_click() {
    let home = tempfile::tempdir().unwrap();
    for command in [
        "ls -lah .",
        "grep -rin 'error' src",
        "grep -E '^(foo|bar)$' file",
        "ls --color=never 'My Projects'",
        "stat Cargo.toml",
        "df -h .",
        "du -sh src",
        "free -h",
        "id -un",
    ] {
        let call = serde_json::json!({"tool_name":"Bash", "tool_input":{"command":command}});
        let printed = super::run(home.path(), "jj", &mut call.to_string().as_bytes());
        let result: serde_json::Value = serde_json::from_str(&printed).unwrap();
        assert_eq!(
            result["hookSpecificOutput"]["permissionDecision"], "allow",
            "{command}: {printed}"
        );
    }
}
