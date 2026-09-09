//! Exercise Linux executable replacement, not only a fabricated path string.
use super::{asking, command, executable, shell};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

#[test]
fn child_fixture() {
    if std::env::var_os("CARL_DELETED_EXE_CHILD").is_none() {
        return;
    }
    println!("READY");
    std::io::stdout().flush().unwrap();
    let mut go = [0];
    std::io::stdin().read_exact(&mut go).unwrap();
    let stale = std::env::current_exe().unwrap();
    assert!(
        stale
            .as_os_str()
            .as_encoded_bytes()
            .ends_with(b" (deleted)")
    );
    let settings = asking::for_this_build(std::path::Path::new("/tmp"), "jj").unwrap();
    println!("SETTINGS={settings}");
}

#[test]
fn a_running_unlinked_process_uses_the_installed_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let running = dir.path().join("carl");
    std::fs::copy(std::env::current_exe().unwrap(), &running).unwrap();
    let mut launcher = Command::new(&running);
    launcher
        .args([
            "--exact",
            "claude::asking_regression_tests::deleted_process::child_fixture",
            "--nocapture",
        ])
        .env("CARL_DELETED_EXE_CHILD", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Concurrent test processes can briefly inherit the copy writer before exec.
    let mut attempts = 0;
    let mut child = loop {
        match launcher.spawn() {
            Ok(child) => break child,
            Err(error) if error.raw_os_error() == Some(26) && attempts < 20 => {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => panic!("could not start replacement fixture: {error}"),
        }
    };
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "child did not become ready"
        );
        if line.trim() == "READY" {
            break;
        }
    }
    let replacement = dir.path().join("replacement");
    executable(&replacement, "#!/bin/sh\nprintf '%s\\n' REPLACEMENT\n");
    std::fs::rename(replacement, &running).unwrap();
    child.stdin.take().unwrap().write_all(b"g").unwrap();
    let mut result = String::new();
    reader.read_to_string(&mut result).unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "{result}");
    let settings = result
        .lines()
        .find_map(|line| line.strip_prefix("SETTINGS="))
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(settings).unwrap();
    let generated = parsed["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert!(!generated.contains("(deleted)"));
    assert_eq!(
        generated,
        command(&running, std::path::Path::new("/tmp"), "jj")
    );
    let output = shell(generated, dir.path());
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "REPLACEMENT"
    );
}
