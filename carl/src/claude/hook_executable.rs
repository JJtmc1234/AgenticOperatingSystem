//! Prefer the configured filesystem path over a stale process executable link.
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

pub(super) fn clean(path: &Path) -> PathBuf {
    let bytes = path.as_os_str().as_bytes();
    PathBuf::from(OsString::from_vec(
        bytes.strip_suffix(b" (deleted)").unwrap_or(bytes).to_vec(),
    ))
}

pub(super) fn resolve(
    configured: Option<&OsStr>,
    fallback: Option<&Path>,
    search: Option<&OsStr>,
    cwd: &Path,
) -> Option<PathBuf> {
    if let Some(configured) = configured {
        let path = clean(Path::new(configured));
        if path.is_absolute() || path.components().count() > 1 {
            let path = if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            };
            if path.is_file() {
                return Some(path);
            }
        } else if let Some(search) = search {
            for directory in std::env::split_paths(search) {
                let directory = if directory.is_absolute() {
                    directory
                } else {
                    cwd.join(directory)
                };
                let candidate = directory.join(&path);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    fallback.map(clean)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_path_wins_over_the_deleted_proc_link() {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join("configured carl");
        std::fs::write(&installed, "replacement").unwrap();
        assert_eq!(
            resolve(
                Some(installed.as_os_str()),
                Some(Path::new("/old/carl (deleted)")),
                None,
                dir.path()
            ),
            Some(installed)
        );
    }
    #[test]
    fn bare_configured_name_is_resolved_from_path() {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join("carl");
        std::fs::write(&installed, "replacement").unwrap();
        assert_eq!(
            resolve(
                Some(OsStr::new("carl")),
                None,
                Some(dir.path().as_os_str()),
                dir.path()
            ),
            Some(installed)
        );
    }
    #[test]
    fn a_missing_replacement_still_has_a_clean_diagnostic_path() {
        assert_eq!(
            resolve(
                None,
                Some(Path::new("/missing/carl (deleted)")),
                None,
                Path::new("/")
            ),
            Some(PathBuf::from("/missing/carl"))
        );
    }
}
