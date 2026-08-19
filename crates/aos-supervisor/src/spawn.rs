//! Launching one process safely.
//!
//! Split from the supervisor because these are two different jobs. This decides how a
//! process is allowed to come into existence. The supervisor tracks a set of them.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use aos_core::{AgentSpec, Allowlist, Error, ProcessHandle, Result};

use crate::proc;

/// Checks the program against the allowlist, then launches it.
///
/// Returns the child alongside the handle that identifies it, because a pid on its own stops
/// meaning anything the moment the supervisor is not there to watch it.
pub fn launch(
    spec: &AgentSpec,
    allowed: &Allowlist,
    log_dir: &Path,
    log_file: &Path,
) -> Result<(Child, ProcessHandle, PathBuf)> {
    // The resolved path, not the spelling that was asked for. Spawning the string would let
    // `$PATH` or the working directory pick the file, which is the gate naming one thing and
    // the kernel running another. See bug 7.
    let program = allowed.resolve_program(&spec.program)?;

    let log = open_log(log_dir, log_file)?;

    // Arguments go straight to execve as a list. No shell, so a semicolon or a pipe inside
    // an argument stays literal text.
    let child = Command::new(&program)
        .args(&spec.args)
        // The child must not inherit our stdin. On the Windows AOS a child inherited the
        // protocol pipe and ate the parent's messages.
        .stdin(Stdio::null())
        // A file, never a pipe. Nothing here drains a pipe, and a full pipe buffer stops the
        // agent dead at roughly 64 KB of output. See bug 1 in bug-list.md.
        .stderr(Stdio::from(log.try_clone()?))
        .stdout(Stdio::from(log))
        .spawn()?;

    let pid = child.id();

    // Read the start time now, while the child is certainly ours. A process that exits
    // immediately becomes a zombie and keeps its /proc entry until it is reaped, so this
    // still works for something as short lived as `true`.
    let start_token = proc::start_token(pid).ok_or_else(|| {
        Error::Refused(format!(
            "{} vanished before it could be identified",
            spec.id
        ))
    })?;

    Ok((child, ProcessHandle { pid, start_token }, program))
}

/// Opens an agent's log for appending, creating the directory on first use.
///
/// Append rather than truncate, so restarting an agent keeps the history that explains why
/// it needed restarting.
fn open_log(dir: &Path, path: &Path) -> Result<std::fs::File> {
    std::fs::create_dir_all(dir)?;
    Ok(std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?)
}
