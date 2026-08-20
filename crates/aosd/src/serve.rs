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

pub fn run(run_dir: &Path) -> Result<()> {
    // Bound before anything is booted, and the order is the whole point.
    //
    // `bind` is what enforces one daemon per run directory. `boot` replays the log and appends
    // a `lost_while_unsupervised` record for every agent it finds gone. Booting first meant a
    // second `aosd` wrote those records and only then discovered a live daemon and exited, so
    // it had already spent sequence numbers the live daemon believed were still free. The live
    // daemon's cached `next_seq` was then permanently behind, and every record it wrote from
    // that point collided with one already in the file. See bug 8.
    //
    // Nothing touches the ledger until this process has proved it is the only supervisor here.
    let socket = Daemon::socket_path(run_dir);
    let listener = listen::bind(&socket)?;
    let mut daemon = Daemon::boot(run_dir)?;

    let shutdown = install_signal_handler()?;
    eprintln!("aosd listening on {}", socket.display());

    // A timeout on accept, so a quiet daemon still notices a shutdown signal rather than
    // blocking in the kernel until someone connects.
    listener.set_nonblocking(true)?;

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                if let Err(e) = session(&mut daemon, stream) {
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
fn session(daemon: &mut Daemon, stream: UnixStream) -> Result<()> {
    if !listen::peer_is_owner(&stream) {
        // Say nothing useful and close. An unknown caller learns only that something is here.
        return Ok(());
    }

    let mut writer = stream.try_clone()?;
    let reader = BufReader::new(stream);

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        // A malformed request is answered, not dropped. A caller that gets silence cannot
        // tell a rejection from a crash.
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => daemon.handle(request),
            Err(e) => Response::error(format!("bad request: {e}")),
        };

        writeln!(writer, "{}", serde_json::to_string(&response)?)?;
        writer.flush()?;
    }
    Ok(())
}

/// Sets a flag on SIGINT and SIGTERM so the loop can wind down.
///
/// The handler only stores a bool. Anything more in a signal handler risks running where it
/// is not safe to, and the loop checks the flag often enough.
fn install_signal_handler() -> Result<Arc<AtomicBool>> {
    let flag = Arc::new(AtomicBool::new(false));

    for signal in [libc::SIGINT, libc::SIGTERM] {
        // Safety: installs a handler that only writes one atomic, which is async signal safe.
        unsafe {
            libc::signal(signal, handle_signal as *const () as libc::sighandler_t);
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
