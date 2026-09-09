//! Automatic approval for a deliberately small set of observation commands.

use super::permission::{Verdict, decision};

pub(super) fn decision_for(surface: &str, call: &serde_json::Value) -> Option<String> {
    if !matches!(surface, "jj" | "slack" | "code") && crate::army::org::find(surface).is_none() {
        return None;
    }
    if call["tool_name"].as_str()? != "Bash" {
        return None;
    }
    let command = call["tool_input"]["command"].as_str()?;
    harmless(command).then(|| {
        decision(
            Verdict::Allow,
            "Automatically approved an inspection command with validated arguments",
        )
    })
}

fn harmless(command: &str) -> bool {
    if super::safe_grep::accepts(command) {
        return true;
    }
    // Only literal path characters and quotes. Never expansion or composition.
    if command.len() > 4096
        || !command.bytes().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    b' ' | b'\t' | b'/' | b'-' | b'.' | b'_' | b'=' | b'\'' | b'"'
                )
        })
    {
        return false;
    }
    let Some(raw_program) = command.split_ascii_whitespace().next() else {
        return false;
    };
    // Keep executable selection literal, including refusal of quoted wrappers.
    if raw_program.contains(['\'', '"']) {
        return false;
    }
    let Some(words) = shlex::split(command) else {
        return false;
    };
    if words.len() > 128 {
        return false;
    }
    let program = raw_program
        .strip_prefix("/usr/bin/")
        .or_else(|| raw_program.strip_prefix("/bin/"))
        .unwrap_or(raw_program);
    if let Some(accepted) = super::safe_metadata::accepts(program, &words[1..]) {
        return accepted;
    }
    let options: &[&str] = match program {
        "pwd" => &["-L", "-P", "--logical", "--physical"],
        "whoami" | "hostname" => &[],
        "uname" => &[
            "-a", "-s", "-n", "-r", "-v", "-m", "-p", "-i", "-o", "--all",
        ],
        "date" => &["-u", "--utc", "--universal"],
        "uptime" => &["-p", "-s", "--pretty", "--since"],
        _ => return false,
    };
    words[1..]
        .iter()
        .all(|word| options.contains(&word.as_str()))
}

#[cfg(test)]
#[path = "safe_commands_tests.rs"]
mod tests;
