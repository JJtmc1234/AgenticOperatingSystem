//! Reading process identity out of `/proc`.
//!
//! Exists for one reason. Linux reuses pids, so after a restart the pid the log remembers
//! may belong to a completely unrelated process. Acting on it would mean stopping a stranger.

use aos_core::ProcessHandle;

/// Start time of a process in clock ticks since boot, field 22 of `/proc/<pid>/stat`.
///
/// Returns `None` when the process is gone, which is the common case and not an error.
pub fn start_token(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_start_token(&stat)
}

/// Whether the pid in this handle is still the same process it was when recorded.
///
/// False means either the process ended or its number was recycled. Both mean the same
/// thing to a caller: do not touch that pid.
pub fn is_still(handle: ProcessHandle) -> bool {
    start_token(handle.pid) == Some(handle.start_token)
}

/// Parses field 22 out of a `/proc/<pid>/stat` line.
///
/// Field 2 is the executable name in parentheses and may itself contain spaces and
/// parentheses, so splitting the whole line on whitespace is wrong. Everything after the
/// last `)` is fixed width, and field 3 starts there, which puts field 22 at index 19.
fn parse_start_token(stat: &str) -> Option<u64> {
    let after_comm = &stat[stat.rfind(')')? + 1..];
    after_comm.split_whitespace().nth(19)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real line, taken from `sleep 300` on this machine.
    const REAL: &str = "117911 (sleep) S 117900 117911 117900 0 -1 4194304 488 0 0 0 0 0 0 0 20 0 1 0 9219785 16502784 1920 18446744073709551615 96949347926016";

    #[test]
    fn reads_field_22_from_a_real_stat_line() {
        assert_eq!(parse_start_token(REAL), Some(9_219_785));
    }

    /// The regression this parser exists for. A process named `we ) love ) parens` breaks
    /// any parser that splits the whole line on whitespace.
    #[test]
    fn survives_a_process_name_full_of_spaces_and_parens() {
        let nasty = REAL.replacen("(sleep)", "(we ) love ) parens)", 1);
        assert_eq!(parse_start_token(&nasty), Some(9_219_785));
    }

    #[test]
    fn refuses_a_line_that_is_not_stat() {
        assert_eq!(parse_start_token("total nonsense"), None);
        assert_eq!(parse_start_token(""), None);
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
