use super::*;
use serde_json::json;

#[test]
fn inspection_options_are_explicit() {
    for command in [
        "pwd",
        " pwd\t-P ",
        "/bin/pwd -L",
        "whoami",
        "hostname",
        "uname -a",
        "uname -s -r",
        "date --utc",
        "uptime --since",
    ] {
        assert!(harmless(command), "{command}");
    }
}

#[test]
fn composition_and_mutating_options_never_auto_approve() {
    for command in [
        "",
        "pwd; rm file",
        "pwd && rm file",
        "pwd || rm file",
        "pwd | sh",
        "pwd > file",
        "pwd >> file",
        "pwd < file",
        "pwd\nrm file",
        "pwd\rwhoami",
        "pwd &",
        "pwd $(touch file)",
        "pwd `touch file`",
        "$(pwd)",
        "env pwd",
        "sudo pwd",
        "sh -c pwd",
        "command pwd",
        "./pwd",
        "/tmp/pwd",
        "/usr/bin/../tmp/pwd",
        "PATH=/tmp pwd",
        "pwd\\\n",
        "'pwd'",
        "pwd # comment",
        "date -s tomorrow",
        "date --set tomorrow",
        "date -f commands",
        "hostname new-name",
        "uname --evil",
        "pwd /tmp",
        "whoami --anything",
        "uptime -K",
        "rm -rf /",
        "git status",
    ] {
        assert!(!harmless(command), "{command}");
    }
}

#[test]
fn malformed_calls_and_unknown_surfaces_do_not_get_an_override() {
    for call in [
        json!({}),
        json!({"tool_name":"Read"}),
        json!({"tool_name":"Bash","tool_input":{"command":42}}),
    ] {
        assert!(decision_for("jj", &call).is_none());
    }
    let call = json!({"tool_name":"Bash","tool_input":{"command":"pwd"}});
    assert!(decision_for("unknown-agent", &call).is_none());
}

#[test]
fn file_inspection_accepts_literal_paths_and_bounded_options() {
    for command in [
        "ls -lah .",
        "/usr/bin/ls --color=never \"My Projects\"",
        "ls -- -filename",
        "stat -Lt Cargo.toml",
        "df -h /home",
        "du -sh src",
        "du --max-depth=2 .",
        "free -h",
        "id -un",
    ] {
        assert!(harmless(command), "{command}");
    }
}

#[test]
fn file_inspection_refuses_execution_tricks_and_unlisted_options() {
    for command in [
        "ls; touch file",
        "ls $(touch file)",
        "ls `touch file`",
        "ls > output",
        "ls | sh",
        "ls &",
        "ls *",
        "ls ~",
        "ls \"$HOME\"",
        "ls 'unterminated",
        "ls --color=always",
        "ls --hide=file",
        "stat --printf=anything .",
        "df --sync",
        "du --files0-from=commands",
        "du --max-depth=999",
        "du --max-depth=-1",
        "free -s 1",
        "id --evil",
        "/tmp/ls",
        "./ls",
        "env ls",
        "ls\nrm file",
    ] {
        assert!(!harmless(command), "{command}");
    }
    assert!(!harmless(&format!("ls {}", "x".repeat(4096))));
    assert!(!harmless(&format!("ls {}", "x ".repeat(128))));
}

#[test]
fn common_grep_searches_are_approved_without_shell_execution() {
    for command in [
        "grep error file.log",
        "grep -rin 'error' src",
        "grep -E '^(foo|bar)$' file",
        "grep -n -A 3 -- 'TODO.*' file",
        "/usr/bin/grep --include='*.rs' -r TODO src",
    ] {
        assert!(harmless(command), "{command}");
    }
    for command in [
        "grep x file; rm file",
        "grep x file | sh",
        "grep $(touch file) file",
        "grep \"$HOME\" file",
        "grep x file > output",
        "grep --evil x file",
        "grep -A nope x file",
        "grep x\nrm file",
        "/tmp/grep x file",
    ] {
        assert!(!harmless(command), "{command}");
    }
}
