//! What tier a launch is judged at, worked out from the program rather than asked of the caller.
//!
//! The gate used to take the tier straight out of the request. A caller that picks its own tier
//! picks its own verdict, so the plan and commit handshake was optional for anybody willing to
//! type a different word. Two byte identical launches of `/usr/bin/rm` differing only in
//! `"ceiling"` answered `plan_required` for `destructive` and `started` for `read`, the second
//! one deleting the directory with no plan, no commit and no human, and the log recorded only
//! that something started. See bug 34.
//!
//! The table is names rather than paths, so `/bin/rm` and `/usr/bin/rm` are the same program,
//! which they are on every machine this runs on. It is short and it is meant to be edited: a
//! program nobody has classified lands on the safe side rather than the convenient one.

use std::path::Path;

use crate::RiskTier;

/// Observes the machine and changes nothing.
const READ: &[&str] = &[
    "true",
    "false",
    "echo",
    "sleep",
    "cat",
    "head",
    "tail",
    "ls",
    "date",
    "pwd",
    "wc",
    "uname",
    "hostname",
    "printenv",
    "df",
    "du",
    "ps",
    "uptime",
    "seq",
    "grep",
    "diff",
    "stat",
    "file",
    "basename",
    "dirname",
    "sort",
    "uniq",
    "cut",
    "md5sum",
    "sha256sum",
];

/// Changes data the user owns and could restore.
const WRITE: &[&str] = &[
    "cp", "mv", "touch", "mkdir", "tee", "tar", "gzip", "gunzip", "zip", "unzip", "patch", "make",
    "cargo", "rsync", "curl", "wget", "git",
];

/// Changes machine state outside the user's own files.
const SYSTEM: &[&str] = &[
    "systemctl",
    "service",
    "mount",
    "umount",
    "chmod",
    "chown",
    "chgrp",
    "ln",
    "kill",
    "killall",
    "pkill",
    "ip",
    "iptables",
    "nft",
    "sysctl",
    "modprobe",
    "useradd",
    "usermod",
    "groupadd",
    "crontab",
    "apt",
    "apt-get",
    "dpkg",
    "snap",
];

/// Loses something that cannot be brought back, or hands out everything.
const DESTRUCTIVE: &[&str] = &[
    "rm", "rmdir", "shred", "dd", "mkfs", "fdisk", "parted", "wipefs", "shutdown", "reboot",
    "halt", "poweroff",
    // Raising privilege is the top tier whatever the command after it is, because the command
    // after it is no longer bounded by anything here.
    "sudo", "su", "doas", "pkexec",
    // Interpreters and shells. Each takes code on its own argument vector, so allowing one
    // grants everything every other gate protects. They should never reach an allowlist at all,
    // and CLAUDE.md says so. If one ever does, this makes sure it needs a human every time
    // rather than inheriting whatever tier the caller felt like claiming.
    "sh", "bash", "dash", "zsh", "fish", "ksh", "python", "python3", "perl", "ruby", "node", "deno",
    "awk", "gawk",
    // Not interpreters themselves, and they run whatever they are handed, which comes to the
    // same thing.
    "env", "xargs", "find", "nohup", "setsid", "timeout",
];

/// The tier a launch is judged at, from the program that would run.
///
/// Arguments are deliberately not consulted. A classifier over argument strings looks like a
/// gate and is not one: alternative spellings, quoting, `--`, long and short flags and an
/// environment prefix all reach the same effect under a different string, and every near miss
/// reads as allowed. A tier is a property of what the binary can do, and the binary is the thing
/// the allowlist names and a human wrote down.
pub fn tier_of(program: &str) -> RiskTier {
    let name = Path::new(program)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let name = name.as_str();

    if DESTRUCTIVE.contains(&name) {
        RiskTier::Destructive
    } else if SYSTEM.contains(&name) {
        RiskTier::System
    } else if WRITE.contains(&name) {
        RiskTier::Write
    } else if READ.contains(&name) {
        RiskTier::Read
    } else {
        // Unknown is not harmless. Read means "changes nothing", and nothing here has
        // established that about a program it has never heard of, so answering Read would be
        // the same mistake as trusting the caller, made by this file instead.
        //
        // System rather than Destructive, because the top tier should mean this particular
        // thing loses data that cannot come back. Flattening the two would make it stop meaning
        // anything, and a tier nobody believes is one somebody widens. Under the default policy
        // System prompts, so an unknown program gets a plan and a person, not a refusal and not
        // a free pass.
        RiskTier::System
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug, at its narrowest. The tier must not be something the caller can choose.
    #[test]
    fn the_tier_comes_from_the_program() {
        assert_eq!(tier_of("/usr/bin/rm"), RiskTier::Destructive);
        assert_eq!(tier_of("/usr/bin/sleep"), RiskTier::Read);
        assert_eq!(tier_of("/usr/bin/mkdir"), RiskTier::Write);
        assert_eq!(tier_of("/usr/bin/systemctl"), RiskTier::System);
    }

    /// The same file reached by either of the paths it lives at on a normal machine.
    #[test]
    fn the_directory_a_program_sits_in_does_not_change_what_it_is() {
        assert_eq!(tier_of("/bin/rm"), tier_of("/usr/bin/rm"));
        assert_eq!(tier_of("/bin/echo"), tier_of("/usr/bin/echo"));
    }

    /// An unknown program is not a harmless one. Answering Read here would be the same mistake
    /// as trusting the caller, made one file further in.
    #[test]
    fn a_program_nobody_classified_is_not_treated_as_harmless() {
        for unknown in [
            "/opt/agents/morning-brief",
            "/usr/local/bin/whatever",
            "notaprogram",
            "",
        ] {
            assert!(
                tier_of(unknown) > RiskTier::Read,
                "{unknown} was treated as harmless"
            );
        }
    }

    /// An interpreter takes code on its own argument vector, so allowing one grants everything
    /// the other gates protect. It never gets a low tier here, whatever the caller claims.
    #[test]
    fn an_interpreter_or_a_privilege_raise_is_the_top_tier() {
        for danger in [
            "/bin/sh",
            "/bin/bash",
            "/usr/bin/python3",
            "/usr/bin/node",
            "/usr/bin/sudo",
            "/usr/bin/env",
        ] {
            assert_eq!(tier_of(danger), RiskTier::Destructive, "{danger}");
        }
    }
}
