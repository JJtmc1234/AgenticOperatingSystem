//! Graceful stop, then a hard stop.
//!
//! Split out from the supervisor because this is the only part that reaches past the
//! standard library, and it should be readable on its own.

use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

use aos_core::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopMode {
    /// SIGTERM. The agent may clean up.
    Graceful,
    /// SIGKILL. The agent gets no say.
    Forced,
}

/// Sends SIGTERM, waits up to `grace`, then sends SIGKILL and waits again.
///
/// Returns `None` only if the child survives both, which on Linux means it is stuck in
/// uninterruptible sleep rather than ignoring us.
pub fn stop_child(child: &mut Child, grace: Duration) -> Result<Option<ExitStatus>> {
    term(child);
    if let Some(status) = wait_bounded(child, grace)? {
        return Ok(Some(status));
    }

    child.kill()?;
    wait_bounded(child, Duration::from_secs(5))
}

#[cfg(unix)]
fn term(child: &Child) {
    // Safety: kill on a pid we own and have not reaped. The worst case is ESRCH, which we
    // ignore because the child exiting first is a normal race and not an error.
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
}

#[cfg(not(unix))]
fn term(_child: &Child) {
    // No SIGTERM equivalent. stop_child falls through to kill.
}

/// Polls until the child exits or the deadline passes.
///
/// Polling rather than blocking on wait, because a blocking wait cannot be given a timeout
/// without a second thread, and an unbounded wait is the bug this function exists to avoid.
fn wait_bounded(child: &mut Child, limit: Duration) -> Result<Option<ExitStatus>> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
