//! Talking to `aosd`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use aos_core::{Request, Response};

pub fn socket_path(run_dir: &Path) -> PathBuf {
    run_dir.join("aosd.sock")
}

/// Sends one request and reads one response.
///
/// A missing socket is reported as "no daemon" rather than as a file error, because that is
/// what it means to the person typing and it names the fix.
pub fn ask(run_dir: &Path, request: &Request) -> Result<Response> {
    let path = socket_path(run_dir);
    let stream = UnixStream::connect(&path).map_err(|e| {
        anyhow!(
            "no daemon answering on {} ({e}). Start one with: aosd --run-dir {}",
            path.display(),
            run_dir.display()
        )
    })?;

    let mut writer = stream.try_clone()?;
    writeln!(writer, "{}", serde_json::to_string(request)?)?;
    writer.flush()?;

    let mut line = String::new();
    let read = BufReader::new(stream).read_line(&mut line)?;
    if read == 0 {
        bail!("the daemon closed the connection without answering");
    }

    Ok(serde_json::from_str(&line)?)
}

/// Sends a request and turns an `Error` response into a real error.
///
/// Most callers want a refusal to stop them, and the ones that want to inspect it can use
/// `ask` directly.
pub fn demand(run_dir: &Path, request: &Request) -> Result<Response> {
    match ask(run_dir, request)? {
        Response::Error { message } => bail!(message),
        other => Ok(other),
    }
}
