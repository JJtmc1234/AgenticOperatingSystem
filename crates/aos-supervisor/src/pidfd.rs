//! A handle to one process that cannot be confused with any other.
//!
//! The start token in `proc` tells you whether a pid is still the process you meant. That is
//! enough to decide, but not enough to act on. Between checking and sending a signal the
//! process can exit and its number can be handed to something else, so the check passes and
//! the signal still lands on a stranger. The window is tiny and the consequence is killing
//! the wrong program, which is exactly the kind of bug that never reproduces.
//!
//! A pidfd closes it. The kernel pins the process behind a file descriptor, and that
//! descriptor never comes to mean a different process. Signals sent through it either reach
//! the process we opened or fail because it is gone.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

use aos_core::ProcessHandle;

use crate::proc;

pub struct PidFd {
    fd: OwnedFd,
    handle: ProcessHandle,
}

impl PidFd {
    /// Pins the process named by `handle`, or returns `None` if that process is not there.
    ///
    /// Check, open, then check again. The second check is the whole point. If the token
    /// still matches while we already hold the descriptor, then the descriptor belongs to
    /// the process we meant, because a pidfd cannot start referring to a different one after
    /// it is open. Without the recheck, the process could have been replaced in the gap
    /// between the first check and the open.
    pub fn open(handle: ProcessHandle) -> Option<Self> {
        if !proc::is_still(handle) {
            return None;
        }

        // Safety: a plain syscall with a pid and no flags. It returns a new descriptor or a
        // negative error code, and touches nothing of ours either way.
        let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, handle.pid as libc::pid_t, 0) };
        if raw < 0 {
            return None;
        }

        // Safety: the syscall just handed us this descriptor and nothing else owns it.
        let fd = unsafe { OwnedFd::from_raw_fd(raw as RawFd) };

        if !proc::is_still(handle) {
            // Dropping fd closes it. Whatever holds that pid now, it is not ours.
            return None;
        }

        Some(Self { fd, handle })
    }

    pub fn handle(&self) -> ProcessHandle {
        self.handle
    }

    /// Sends a signal to the pinned process.
    pub fn send(&self, signal: libc::c_int) -> std::io::Result<()> {
        // Safety: our own descriptor, a null siginfo meaning "the kernel fills it in", and no
        // flags. The kernel validates the signal number.
        let rc = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.fd.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    /// Whether the process is still running.
    ///
    /// Signal 0 is the standard "do not actually signal, just tell me if you could". It fails
    /// with ESRCH once the process is gone.
    pub fn is_alive(&self) -> bool {
        self.send(0).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn handle_for(pid: u32) -> ProcessHandle {
        ProcessHandle {
            pid,
            start_token: proc::start_token(pid).expect("process should exist"),
        }
    }

    #[test]
    fn opens_and_reports_our_own_process_alive() {
        let me = PidFd::open(handle_for(std::process::id())).expect("should pin ourselves");
        assert!(me.is_alive());
    }

    /// A wrong token means the pid was recycled, so there is nothing safe to pin.
    #[test]
    fn refuses_to_open_a_handle_whose_token_is_wrong() {
        let mut wrong = handle_for(std::process::id());
        wrong.start_token += 1;
        assert!(PidFd::open(wrong).is_none());
    }

    #[test]
    fn refuses_a_pid_that_does_not_exist() {
        assert!(
            PidFd::open(ProcessHandle {
                pid: u32::MAX,
                start_token: 1
            })
            .is_none()
        );
    }

    /// The real job. Pin a process we did not spawn as a child, then stop it through the
    /// descriptor rather than through its number.
    #[test]
    fn kills_a_real_process_through_the_descriptor() {
        let mut child = Command::new("/usr/bin/sleep")
            .arg("60")
            .stdin(Stdio::null())
            .spawn()
            .unwrap();

        let pinned = PidFd::open(handle_for(child.id())).expect("should pin the sleeper");
        assert!(pinned.is_alive());

        pinned.send(libc::SIGKILL).unwrap();

        // Reap it so the pid is genuinely released rather than left as a zombie.
        child.wait().unwrap();
        assert!(!pinned.is_alive(), "the process should be gone");
    }
}
