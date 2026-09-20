use crate::Result;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::Path;

pub(super) fn lock_directory(path: &Path) -> Result<File> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let directory = File::open(parent)?;
    loop {
        // The live directory descriptor remains stable while the registry file is renamed.
        // Every registry writer holds this lock only while reloading and saving its update.
        // The owned File keeps this valid descriptor open throughout the system call.
        if unsafe { libc::flock(directory.as_raw_fd(), libc::LOCK_EX) } == 0 {
            return Ok(directory);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error.into());
        }
    }
}
