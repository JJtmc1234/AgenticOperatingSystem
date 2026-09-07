//! A cancellable pipe reader so inherited stdout cannot trap a completed handoff.
use super::{Chunk, chunk_of};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::process::ChildStdout;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::thread::JoinHandle;

pub struct Reader {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Reader {
    pub fn start(mut output: ChildStdout, sender: Sender<Chunk>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut pending = Vec::new();
            let mut bytes = [0u8; 8192];
            while !stopped.load(Ordering::Acquire) {
                let mut fd = libc::pollfd {
                    fd: output.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // The pipe has one reader and remains owned until this thread exits.
                let ready = unsafe { libc::poll(&mut fd, 1, 100) };
                if ready < 0 {
                    break;
                }
                if ready == 0 {
                    continue;
                }
                let count = match output.read(&mut bytes) {
                    Ok(n) => n,
                    Err(_) => break,
                };
                if count == 0 {
                    if let Ok(line) = std::str::from_utf8(&pending)
                        && let Some(chunk) = chunk_of(line)
                    {
                        let _ = sender.send(chunk);
                    }
                    break;
                }
                pending.extend_from_slice(&bytes[..count]);
                while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                    let line: Vec<_> = pending.drain(..=end).collect();
                    if let Ok(line) = std::str::from_utf8(&line)
                        && let Some(chunk) = chunk_of(line)
                        && sender.send(chunk).is_err()
                    {
                        return;
                    }
                }
                if pending.len() > 1024 * 1024 {
                    break;
                }
            }
        });
        Self { stop, thread }
    }

    pub fn stop(self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.thread.join();
    }
}

pub fn finish(child: &mut std::process::Child) -> Option<std::process::ExitStatus> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Err(_) => return None,
            Ok(None) => {}
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = child.kill();
    let until = std::time::Instant::now() + std::time::Duration::from_millis(200);
    while std::time::Instant::now() < until {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    None
}
