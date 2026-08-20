//! Reading process identity out of `/proc`.
//!
//! Exists for one reason. Linux reuses pids, so after a restart the pid the log remembers
//! may belong to a completely unrelated process. Acting on it would mean stopping a stranger.

use aos_core::ProcessHandle;

/// What `/proc` was able to say about a pid.
///
/// Three answers rather than two, and the third is the point. "The process is gone" and "I
/// could not find out" look identical to a caller that only has an `Option`, and they call for
/// opposite actions: the first closes the log's hole, the second must leave it open. See bug 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Started {
    /// Read successfully. This is the start time in clock ticks since boot.
    At(u64),
    /// The process is not there, which is the common case and not an error.
    Gone,
    /// Something stopped the read that is not the process being gone.
    Unknown,
}

/// Start time of a process in clock ticks since boot, field 22 of `/proc/<pid>/stat`.
pub fn started(pid: u32) -> Started {
    // Bytes, not a `String`. The kernel escapes only newline and backslash in the comm field,
    // so a process named with bytes above 0x7f produces a stat line that is not valid UTF-8,
    // and `read_to_string` fails on it. That failure used to be indistinguishable from the
    // process being gone, so an agent could escape the kill switch by renaming itself.
    let raw = match std::fs::read(format!("/proc/{pid}/stat")) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Started::Gone,
        // Descriptor exhaustion, a permissions change, anything else. Not knowing is not the
        // same as knowing it is gone.
        Err(_) => return Started::Unknown,
    };

    match parse_start_token(&raw) {
        Some(token) => Started::At(token),
        // The file was there and did not have field 22 in it. That should not happen, and if
        // it does, guessing is the one thing not to do.
        None => Started::Unknown,
    }
}

/// Start time, or `None` for a process that is gone or cannot be read about.
///
/// Kept for callers that genuinely have nothing useful to do with the difference. Anything
/// deciding whether to write a process off should use `started` instead.
pub fn start_token(pid: u32) -> Option<u64> {
    match started(pid) {
        Started::At(token) => Some(token),
        Started::Gone | Started::Unknown => None,
    }
}

/// Whether the pid in this handle is still the same process it was when recorded.
///
/// False means the process ended, its number was recycled, or `/proc` could not be read. All
/// three mean the same thing to a caller about to send a signal: do not touch that pid. A
/// caller deciding whether an agent is *lost* needs `started` instead, because writing an
/// agent off is not the same as declining to signal it.
pub fn is_still(handle: ProcessHandle) -> bool {
    started(handle.pid) == Started::At(handle.start_token)
}

/// Parses field 22 out of a `/proc/<pid>/stat` line.
///
/// Field 2 is the executable name in parentheses and may itself contain spaces and
/// parentheses, so splitting the whole line on whitespace is wrong. Everything after the
/// last `)` is fixed width, and field 3 starts there, which puts field 22 at index 19.
fn parse_start_token(stat: &[u8]) -> Option<u64> {
    // Searched in the bytes, so a comm field that is not UTF-8 does not stop this. Everything
    // after the last `)` is fixed width ASCII, so it converts cleanly once the name is behind.
    let close = stat.iter().rposition(|b| *b == b')')?;
    let after_comm = std::str::from_utf8(&stat[close + 1..]).ok()?;
    after_comm.split_whitespace().nth(19)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug. The kernel escapes only newline and backslash in the comm field, so a process
    /// named with bytes above 0x7f produces a stat line that is not valid UTF-8.
    /// `read_to_string` failed on it, `start_token` gave `None`, and `None` meant "gone". So a
    /// live agent was written off, dropped from `believed_running` on every later boot, and put
    /// beyond the reach of `stop` and of `stop-all`.
    ///
    /// An agent that wanted to survive the kill switch only had to rename itself.
    #[test]
    fn a_name_that_is_not_utf8_does_not_hide_the_start_time() {
        // The real shape: pid, then a comm field with high bytes in it, then the fixed width
        // fields, with field 22 at index 19 after the closing parenthesis.
        let mut line: Vec<u8> = b"117911 (\xff\xfe-agent) S".to_vec();
        line[8] = 0xff;
        line[9] = 0xfe;
        let tail = &REAL.as_bytes()[REAL.rfind(')').unwrap() + 1..];
        line.truncate(line.len() - 2);
        line.extend_from_slice(b")");
        line.extend_from_slice(tail);

        assert!(
            std::str::from_utf8(&line).is_err(),
            "this test is pointless unless the line really is not utf8"
        );
        assert_eq!(parse_start_token(&line), Some(9_219_785));
    }

    /// A pid that is not there is gone, which is ordinary and not an error.
    #[test]
    fn a_pid_that_does_not_exist_is_gone() {
        // Above the usual pid_max, so it is not there and is not about to be reused mid test.
        assert_eq!(started(u32::MAX - 1), Started::Gone);
    }

    /// And this process is certainly still here, which is what stops the fix reading as
    /// "everything is unknown now".
    #[test]
    fn a_live_pid_reads_back_a_start_time() {
        match started(std::process::id()) {
            Started::At(t) => assert!(t > 0),
            other => panic!("this process is running, got {other:?}"),
        }
    }

    /// A real line, taken from `sleep 300` on this machine.
    const REAL: &str = "117911 (sleep) S 117900 117911 117900 0 -1 4194304 488 0 0 0 0 0 0 0 20 0 1 0 9219785 16502784 1920 18446744073709551615 96949347926016";

    #[test]
    fn reads_field_22_from_a_real_stat_line() {
        assert_eq!(parse_start_token(REAL.as_bytes()), Some(9_219_785));
    }

    /// The regression this parser exists for. A process named `we ) love ) parens` breaks
    /// any parser that splits the whole line on whitespace.
    #[test]
    fn survives_a_process_name_full_of_spaces_and_parens() {
        let nasty = REAL.replacen("(sleep)", "(we ) love ) parens)", 1);
        assert_eq!(parse_start_token(nasty.as_bytes()), Some(9_219_785));
    }

    #[test]
    fn refuses_a_line_that_is_not_stat() {
        assert_eq!(parse_start_token(b"total nonsense"), None);
        assert_eq!(parse_start_token(b""), None);
    }

    #[test]
    fn reads_our_own_process() {
        let me = std::process::id();
        assert!(start_token(me).is_some(), "cannot read /proc/{me}/stat");
    }

    #[test]
    fn a_pid_that_does_not_exist_has_no_token() {
        // Above /proc/sys/kernel/pid_max on any normal system.
        assert_eq!(start_token(u32::MAX), None);
    }

    /// The guard itself. Same pid, wrong token, means a recycled number.
    #[test]
    fn a_wrong_token_means_a_different_process() {
        let me = std::process::id();
        let real = start_token(me).unwrap();

        assert!(is_still(ProcessHandle {
            pid: me,
            start_token: real
        }));
        assert!(!is_still(ProcessHandle {
            pid: me,
            start_token: real + 1
        }));
    }
}
