//! Telling Claude Code to ask before it acts.
//!
//! The CLI decides a tool call by running `PreToolUse` hooks and obeying what they print. So
//! making Carl ask JJ is a matter of installing one, and the hook is Carl himself under
//! `permit-hook`.
//!
//! Settings are supplied as JSON. Executable replacement must not turn a stale
//! process link into shell syntax, and resolution failures must still install a deny hook.
//!
//! **This adds a hook, it does not replace the ones JJ has.** `--settings` loads additional
//! settings, so `guard.sh` in `~/.claude/settings.json` still runs on every Bash call. Two hooks
//! run on the same call and either can refuse. That is the intended arrangement: the guard knows
//! about destructive commands and does not need a person, and this one knows nothing and asks.

use std::path::Path;

use serde_json::json;

#[path = "hook_command.rs"]
mod command;
#[path = "hook_executable.rs"]
mod executable;

/// How long the CLI waits for the hook, in seconds.
///
/// Must be longer than the hook's own wait, or the CLI gives up first and the answer arrives
/// with nowhere to go. The hook refuses at ten minutes, so this allows for that plus the time
/// to connect and print.
const CLI_TIMEOUT: u64 = 660;

/// Every tool, rather than only the dangerous looking ones.
///
/// `guard.sh` matches Bash alone, which is right for it: it decides by reading the command. This
/// one decides by asking a person, and a person is the right thing to ask about a write to a
/// path as much as about a shell command.
const EVERY_TOOL: &str = "*";

/// The `--settings` value that makes the CLI ask before every tool call.
///
/// `carl` is the running executable, resolved rather than assumed, so a build in a worktree
/// installs its own hook and not the one in `~/.local/bin`.
pub fn settings(carl: &Path, home: &Path, surface: &str) -> String {
    envelope(command::build(carl, home, surface))
}

fn envelope(command: String) -> String {
    json!({
        "showThinkingSummaries": true,
        "hooks": {
            "PreToolUse": [{
                "matcher": EVERY_TOOL,
                "hooks": [{
                    "type": "command",
                    "command": command,
                    "timeout": CLI_TIMEOUT,
                    "statusMessage": "Asking JJ..."
                }]
            }]
        }
    })
    .to_string()
}

/// Always supplies a hook, including a deny decision when resolution fails.
/// Configured argv and PATH are preferred to the process executable link.
pub fn for_this_build(home: &Path, surface: &str) -> Option<String> {
    let configured = std::env::args_os().next();
    let fallback = std::env::current_exe().ok();
    let search = std::env::var_os("PATH");
    let cwd = std::env::current_dir().unwrap_or_else(|_| "/".into());
    Some(
        match executable::resolve(
            configured.as_deref(),
            fallback.as_deref(),
            search.as_deref(),
            &cwd,
        ) {
            Some(binary) => settings(&binary, home, surface),
            None => envelope(command::deny("carl binary not found at <unresolved>")),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(carl: &str, home: &str) -> serde_json::Value {
        serde_json::from_str(&settings(Path::new(carl), Path::new(home), "jj")).unwrap()
    }

    #[test]
    fn the_hook_runs_this_binary_against_this_home() {
        let v = parsed("/opt/carl/target/debug/carl", "/home/jj_tmc/.carl");
        let command = v["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(
            command.contains(
                "/opt/carl/target/debug/carl --home /home/jj_tmc/.carl permit-hook --as jj"
            )
        );
        assert!(command.contains("carl binary not found at /opt/carl/target/debug/carl"));
    }

    /// A person is the right thing to ask about a file write as much as about a shell command.
    #[test]
    fn it_matches_every_tool_and_not_only_bash() {
        let v = parsed("/c", "/h");
        assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "*");
    }

    /// If the CLI gives up first, the answer arrives with nowhere to go.
    #[test]
    fn the_cli_waits_longer_than_the_hook_does() {
        let v = parsed("/c", "/h");
        let cli = v["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"]
            .as_u64()
            .unwrap();
        assert!(
            cli > crate::panel::permission::WAIT.as_secs(),
            "the CLI waits {cli}s and the hook waits {}s",
            crate::panel::permission::WAIT.as_secs()
        );
    }

    /// The band says who is asking, and a guess from the working directory would put Nora's
    /// name on a question Carl asked.
    #[test]
    fn the_surface_is_carried_rather_than_guessed() {
        let v: serde_json::Value =
            serde_json::from_str(&settings(Path::new("/c"), Path::new("/h"), "nora")).unwrap();
        let command = v["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(command.contains("permit-hook --as nora"), "{command}");
    }
}
