use carl::panel::{hook, permission};

#[test]
fn a_long_unicode_tool_payload_is_denied_without_panicking() {
    let home = tempfile::tempdir().unwrap();
    let payload =
        serde_json::json!({"tool_name":"UnknownTool", "tool_input":{"x":"🙂".repeat(80)}});
    let printed = hook::run(home.path(), "jj", &mut payload.to_string().as_bytes());
    let decision: serde_json::Value = serde_json::from_str(&printed).unwrap();
    assert_eq!(decision["hookSpecificOutput"]["permissionDecision"], "deny");
    let (_, detail) = permission::read_call(&payload);
    assert!(detail.ends_with("..."));
    assert!(detail.len() <= 203);
}
