use super::asking;
use std::io::{Seek, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

fn command(binary: &Path, home: &Path, surface: &str) -> String {
    let value: serde_json::Value =
        serde_json::from_str(&asking::settings(binary, home, surface)).unwrap();
    value["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .into()
}
fn executable(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}
fn shell(text: &str, directory: &Path) -> std::process::Output {
    // File input also works when a missing binary denies before reading stdin.
    let mut input = tempfile::tempfile().unwrap();
    input.write_all(b"{}").unwrap();
    input.rewind().unwrap();
    Command::new("/bin/sh")
        .args(["-c", text])
        .current_dir(directory)
        .stdin(Stdio::from(input))
        .output()
        .unwrap()
}
#[test]
fn hook_paths_and_all_arguments_survive_shell_metacharacters() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("carl's (test)");
    executable(&binary, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n");
    let home = dir.path().join("home ' $(touch PWN)");
    let surface = "jj; touch PWN";
    let output = shell(&command(&binary, &home, surface), dir.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("--home\n{}\npermit-hook\n--as\n{surface}\n", home.display())
    );
    assert!(!dir.path().join("PWN").exists());
}
#[test]
fn deleted_marker_uses_the_replacement_binary_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("carl");
    executable(&binary, "#!/bin/sh\nprintf '%s\\n' REPLACEMENT\n");
    let deleted = dir.path().join("carl (deleted)");
    let script = command(&deleted, dir.path(), "jj");
    assert!(!script.contains("(deleted)"), "{script}");
    let output = shell(&script, dir.path());
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "REPLACEMENT"
    );
}
#[test]
fn missing_binary_returns_explicit_deny_json_with_successful_hook_exit() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing carl (deleted)");
    let output = shell(&command(&missing, dir.path(), "jj"), dir.path());
    assert!(output.status.success());
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reply["hookSpecificOutput"]["permissionDecision"], "deny");
    assert_eq!(
        reply["hookSpecificOutput"]["permissionDecisionReason"],
        format!(
            "carl binary not found at {}",
            dir.path().join("missing carl").display()
        )
    );
}
#[test]
fn an_executable_that_fails_cannot_make_the_guard_disappear() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("carl");
    executable(&binary, "#!/bin/sh\nprintf BROKEN\nexit 17\n");
    let output = shell(&command(&binary, dir.path(), "jj"), dir.path());
    assert!(output.status.success());
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reply["hookSpecificOutput"]["permissionDecision"], "deny");
}

#[path = "asking_deleted_tests.rs"]
mod deleted_process;
