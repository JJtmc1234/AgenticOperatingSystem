use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::path::Path;

pub(super) fn bounded(path: &Path) -> Option<String> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(16 * 1024 * 1024 + 1)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() <= 16 * 1024 * 1024).then_some(text)
}

pub(super) fn busy(root: &Path) -> Option<bool> {
    let file = match File::open(root.join("run.lock")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(false),
        Err(_) => return None,
    };
    // The descriptor stays live through flock. Closing it releases a successful shared lock.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } == 0 {
        return Some(false);
    }
    let e = std::io::Error::last_os_error();
    (e.kind() == std::io::ErrorKind::WouldBlock).then_some(true)
}
