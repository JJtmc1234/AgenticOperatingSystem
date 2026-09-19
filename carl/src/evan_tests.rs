use super::*;
use std::os::unix::fs::PermissionsExt;

fn fixture(dir: &Path, body: &str) -> std::path::PathBuf {
    let path = dir.join("evan fixture");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[test]
fn evan_bridge_preserves_arguments_without_shell_execution() {
    let dir = tempfile::tempdir().unwrap();
    let program = fixture(dir.path(), "printf '%s\\n' \"$@\"");
    let home = dir.path().join("home ' with spaces");
    let request = "quote ' $(touch PWN) ; echo no";
    let args = ["run", "--request", request].map(OsString::from);
    let output = command(&program, &home, &args)
        .unwrap()
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "--home\n{}\nrun\n--request\n{request}\n",
            home.join("evan").display()
        )
    );
    assert!(!dir.path().join("PWN").exists());
}

#[test]
fn evan_bridge_preserves_failure_status_and_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let program = fixture(
        dir.path(),
        "printf 'report\\n'\nprintf 'failed\\n' >&2\nexit 23",
    );
    let output = command(&program, dir.path(), &[])
        .unwrap()
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(output.stdout, b"report\n");
    assert_eq!(output.stderr, b"failed\n");
}

#[test]
fn evan_bridge_missing_install_has_an_actionable_error() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("missing");
    let error = command(&program, dir.path(), &[]).unwrap_err().to_string();
    assert!(error.contains(&program.display().to_string()));
    assert!(error.contains("integrations/evan/install.sh"));
}
