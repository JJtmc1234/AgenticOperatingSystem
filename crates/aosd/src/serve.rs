//! The accept loop.
//!
//! One connection at a time, on purpose. The daemon owns process lifetimes, so two requests
//! racing to start the same agent is a bug with a stray process at the end of it. Serialising
//! is the cheapest way to make that impossible, and nothing here is slow enough to care.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use aos_core::{Request, Response};

use crate::daemon::Daemon;
use crate::listen;

/// How long a read waits before the loop gets a turn to notice a shutdown.
const TICK: std::time::Duration = std::time::Duration::from_millis(200);

pub fn run(run_dir: &Path) -> Result<()> {
    let mut daemon = Daemon::boot(run_dir)?;
    let socket = Daemon::socket_path(run_dir);
    let listener = listen::bind(&socket)?;

    let shutdown = install_signal_handler()?;
    eprintln!("aosd listening on {}", socket.display());

    // A timeout on accept, so a quiet daemon still notices a shutdown signal rather than
    // blocking in the kernel until someone connects.
    listener.set_nonblocking(true)?;

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                if let Err(e) = session(&mut daemon, stream, &shutdown) {
                    eprintln!("connection ended: {e}");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => eprintln!("accept failed: {e}"),
        }
    }

    // Take the socket with us. Leaving it behind makes the next start do a liveness probe it
    // did not need to do.
    let _ = std::fs::remove_file(&socket);
    eprintln!("aosd stopped, agents left running");
    Ok(())
}

/// Serves one connection until the peer hangs up.
fn session(daemon: &mut Daemon, stream: UnixStream, shutdown: &AtomicBool) -> Result<()> {
    if !listen::peer_is_owner(&stream) {
        // Say nothing useful and close. An unknown caller learns only that something is here.
        return Ok(());
    }

    // A read timeout, so a connected client cannot hold the daemon open past a shutdown.
    //
    // This used to block in `read_line` until the peer hung up, and the shutdown flag was only
    // checked between connections. So SIGTERM set the flag and nothing looked at it: one client
    // that connected and said nothing made SIGTERM a no op and left SIGKILL as the only way to
    // stop the daemon. The kill switch is the one thing this design promises always works, and
    // it could not even stop the supervisor. See bug 9.
    stream.set_read_timeout(Some(TICK))?;

    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    loop {
        if shutdown.load(Ordering::Relaxed) {
            return Ok(());
        }

        match reader.read_line(&mut line) {
            // The peer hung up, which is the ordinary way a session ends.
            Ok(0) => return Ok(()),
            Ok(_) => {}
            // A quiet tick. `line` is deliberately kept across it: `read_line` may have taken
            // part of a line before the timeout, and starting again would split the request.
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e.into()),
        }

        let request_line = std::mem::take(&mut line);
        let request_line = request_line.trim();
        if request_line.is_empty() {
            continue;
        }
        let line = request_line;

        // A malformed request is answered, not dropped. A caller that gets silence cannot
        // tell a rejection from a crash.
        let response = match serde_json::from_str::<Request>(line) {
            Ok(request) => daemon.handle(request),
            Err(e) => Response::error(format!("bad request: {e}")),
        };

        writeln!(writer, "{}", serde_json::to_string(&response)?)?;
        writer.flush()?;
    }
}

/// Sets a flag on SIGINT and SIGTERM so the loop can wind down.
///
/// The handler only stores a bool. Anything more in a signal handler risks running where it
/// is not safe to, and the loop checks the flag often enough.
fn install_signal_handler() -> Result<Arc<AtomicBool>> {
    let flag = Arc::new(AtomicBool::new(false));

    // `sigaction` rather than `signal`, and the difference is the whole reason this is four
    // lines instead of one. glibc's `signal` installs the handler with `SA_RESTART`, so a
    // blocked read that a signal interrupts simply resumes rather than returning `EINTR`. The
    // flag was set and the read went straight back to waiting. See bug 9.
    //
    // Deliberately no `SA_RESTART` here, so a read blocked when the signal arrives comes back
    // at once rather than waiting out the timeout as well.
    for signal in [libc::SIGINT, libc::SIGTERM] {
        // Safety: `action` is fully initialised below, and the handler it names only writes one
        // atomic, which is async signal safe.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = handle_signal as *const () as libc::sighandler_t;
            libc::sigemptyset(&mut action.sa_mask);
            action.sa_flags = 0;
            if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
    }

    SHUTDOWN
        .set(flag.clone())
        .map_err(|_| anyhow::anyhow!("signal handler installed twice"))?;
    Ok(flag)
}

static SHUTDOWN: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

extern "C" fn handle_signal(_signal: libc::c_int) {
    if let Some(flag) = SHUTDOWN.get() {
        flag.store(true, Ordering::Relaxed);
    }
}
