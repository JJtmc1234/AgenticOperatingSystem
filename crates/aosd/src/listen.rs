//! Binding the control socket, safely.
//!
//! The socket is the daemon's whole attack surface. Anyone who can talk to it can start and
//! stop processes as this user, so it is owner only, and it refuses to guess about a socket
//! file that is already there.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

use anyhow::{Context, Result, bail};

/// Binds the control socket with owner only permissions.
///
/// A socket file that already exists is either a live daemon or leftover from one that
/// crashed. Guessing is not acceptable in either direction. Deleting a live daemon's socket
/// would strand it, and refusing to start because of a dead one's leftovers would need a
/// manual cleanup every crash. So we ask: if something answers, there is a daemon and we
/// refuse. If nothing answers, the file is debris and we clear it.
pub fn bind(path: &Path) -> Result<UnixListener> {
    if path.exists() {
        match UnixStream::connect(path) {
            Ok(_) => bail!("a daemon is already listening on {}", path.display()),
            Err(_) => {
                std::fs::remove_file(path).with_context(|| {
                    format!("cannot clear the stale socket at {}", path.display())
                })?;
            }
        }
    }

    let dir = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(dir)?;

    // The directory is the outer lock. Without the execute bit nobody else can even reach a
    // name inside it, whatever that name's own permissions say.
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;

    // Bind at the real path, then tighten it.
    //
    // Two cleverer approaches were tried here and both were worse.
    //
    // Narrowing the umask across the bind is wrong because umask is per process rather than
    // per thread, so it changes what every other thread creates at that moment. See bug 4.
    //
    // Binding under a staging name and renaming into place is wrong because a socket path is
    // capped at around 108 bytes, and a staging name is longer than the real one, so it
    // overflows on paths where the real name fits perfectly well. See bug 5.
    //
    // Neither was needed. The directory above is 0700, so in the moment between bind and
    // chmod there is nobody who can reach the socket at all. The directory is the lock.
    let listener =
        UnixListener::bind(path).with_context(|| format!("cannot bind {}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;

    // Assert rather than trust. A filesystem that ignored the chmod must not be served on.
    let mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
    if mode != 0o600 {
        let _ = std::fs::remove_file(path);
        bail!(
            "{} came out as {mode:o} rather than 600, refusing to serve",
            path.display()
        );
    }

    Ok(listener)
}

/// Whether a peer is this same user.
///
/// The file mode already keeps others out, so this is a second lock on the same door. It
/// costs one syscall and it holds even if the permissions are ever loosened by mistake.
pub fn peer_is_owner(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;

    let mut creds = libc::ucred {
        pid: 0,
        uid: u32::MAX,
        gid: u32::MAX,
    };
    let mut len = size_of::<libc::ucred>() as libc::socklen_t;

    // Safety: a getsockopt on our own accepted socket, writing into a local struct whose
    // size we pass alongside it.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut creds).cast::<libc::c_void>(),
            &raw mut len,
        )
    };

    // A failed lookup means we cannot prove who is calling, so we refuse.
    rc == 0 && creds.uid == unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bound_socket_is_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("aosd.sock");

        let _listener = bind(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "socket must not be reachable by anyone else");
    }

    /// Debris from a crashed daemon must not need a manual cleanup.
    #[test]
    fn a_stale_socket_file_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("aosd.sock");

        drop(bind(&path).unwrap());
        assert!(path.exists(), "the file outlives the listener");

        let _second = bind(&path).expect("a dead daemon's leftovers should not block a restart");
    }

    /// A live daemon must never be stranded by a second one deleting its socket.
    #[test]
    fn a_live_socket_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("aosd.sock");

        let _live = bind(&path).unwrap();
        let err = bind(&path).unwrap_err();
        assert!(err.to_string().contains("already listening"));
        assert!(path.exists(), "the live socket must still be there");
    }

    /// Regression test for bug 4.
    ///
    /// `bind` used to narrow the process wide umask across the bind call. Any other thread
    /// creating a directory during that window got one with no execute bit, so its own bind
    /// then failed with permission denied.
    ///
    /// The bug is a race, so the guard has to make its own collisions and has to notice a
    /// narrow one. Two roles rather than one. Binders keep calling `bind`. Watchers do
    /// nothing but create a directory and check its mode, over and over, which samples the
    /// bad window far more often than waiting for a bind of their own to fail.
    ///
    /// The mode a new directory gets depends on this machine's ambient umask, so the baseline
    /// is measured rather than assumed. The invariant is not "directories are 0700", it is
    /// "binding does not change what other threads get". Under the old code's 0o177 they came
    /// out 0600 instead, with no execute bit.
    #[test]
    fn binding_concurrently_does_not_disturb_other_threads() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        // Measured before any binding starts, so it reflects this machine and nothing else.
        let baseline = {
            let dir = tempfile::tempdir().unwrap();
            std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777
        };

        let stop = Arc::new(AtomicBool::new(false));

        let binders: Vec<_> = (0..4)
            .map(|_| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let dir = tempfile::tempdir().unwrap();
                        let _ = bind(&dir.path().join("aosd.sock"));
                    }
                })
            })
            .collect();

        let watchers: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(move || {
                    for _ in 0..2000 {
                        let dir = tempfile::tempdir()?;
                        let mode = std::fs::metadata(dir.path())?.permissions().mode() & 0o777;
                        anyhow::ensure!(
                            mode == baseline,
                            "a directory created during a bind came out {mode:o} instead of \
                             {baseline:o}, so bind is changing process wide state"
                        );
                    }
                    Ok::<(), anyhow::Error>(())
                })
            })
            .collect();

        let mut failure = None;
        for watcher in watchers {
            if let Err(e) = watcher.join().expect("watcher panicked") {
                failure = Some(e);
            }
        }

        stop.store(true, Ordering::Relaxed);
        for binder in binders {
            binder.join().expect("binder panicked");
        }

        if let Some(e) = failure {
            panic!("{e}");
        }
    }

    /// The directory is the outer lock, so it has to be closed even if the socket were not.
    #[test]
    fn the_run_directory_is_owner_only_too() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("run");
        let _listener = bind(&nested.join("aosd.sock")).unwrap();

        let mode = std::fs::metadata(&nested).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "nobody else may even traverse into it");
    }

    /// Regression test for bug 5.
    ///
    /// A unix socket path is capped at about 108 bytes by `sockaddr_un`. The bind used to
    /// happen under a longer staging name, so a run directory whose real socket path fitted
    /// comfortably could still fail. This builds a path close to the limit and checks the
    /// real name is what has to fit.
    #[test]
    fn a_long_run_directory_still_binds() {
        let root = tempfile::tempdir().unwrap();
        let sock_name = "aosd.sock";

        // Grow the directory until the socket path is just inside the limit.
        let mut dir = root.path().to_path_buf();
        while dir.join(sock_name).as_os_str().len() < 100 {
            dir = dir.join("nested");
        }
        let path = dir.join(sock_name);
        assert!(
            path.as_os_str().len() <= 107,
            "test set up a path that cannot work at all: {}",
            path.as_os_str().len()
        );

        let _listener = bind(&path).unwrap_or_else(|e| {
            panic!(
                "a {} byte socket path should bind: {e:#}",
                path.as_os_str().len()
            )
        });
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    /// Nothing extra should be left lying in the run directory.
    #[test]
    fn only_the_socket_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let _listener = bind(&dir.path().join("aosd.sock")).unwrap();

        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["aosd.sock".to_string()]);
    }

    #[test]
    fn our_own_connection_is_recognised_as_ours() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("aosd.sock");
        let listener = bind(&path).unwrap();

        let _client = UnixStream::connect(&path).unwrap();
        let (server_side, _) = listener.accept().unwrap();
        assert!(peer_is_owner(&server_side));
    }
}
