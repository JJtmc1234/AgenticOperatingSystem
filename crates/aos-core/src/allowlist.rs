//! Which programs may be started, resolved to real files rather than to spellings.
//!
//! The allowlist used to be compared by string equality and the same string was then handed to
//! `Command::new`, which is two separate holes. A name with no slash makes `Command::new` search
//! `$PATH` at exec time, so the daemon's inherited environment picked the binary. A relative
//! name with a slash resolves against the daemon's inherited working directory, so the same
//! allowlist file named different binaries depending on where `aosd` happened to be started.
//! Either way the gate named one thing and the kernel ran another. See bug 7.
//!
//! So both sides are canonicalized and the comparison is between real paths. Entries are
//! resolved once at load, and the resolved path is what gets spawned and what gets recorded,
//! which is also what makes the audit log able to say which file actually ran.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Programs that may be launched, each resolved to a real path.
///
/// Never put an interpreter on it. `python -c` and `node -e` take code on their own argument
/// vector, so allowing one grants everything the other gates protect. Resolving the path does
/// nothing about that, because the problem there is what the binary does once it is running.
#[derive(Debug, Clone, Default)]
pub struct Allowlist {
    entries: Vec<PathBuf>,
}

impl Allowlist {
    /// Resolves a set of entries, refusing any that is not an absolute path or does not exist.
    ///
    /// Refusing a relative entry at load rather than at launch is deliberate. A relative entry
    /// is not a narrower permission, it is an ambiguous one, and the moment to reject an
    /// ambiguous rule is before anything has been decided by it.
    pub fn resolve(entries: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut resolved = Vec::new();

        for entry in entries {
            let path = Path::new(&entry);
            if !path.is_absolute() {
                return Err(Error::Refused(format!(
                    "allowlist entry {entry:?} is not an absolute path. A bare name is looked up \
                     on $PATH and a relative one against the working directory, so either would \
                     let the environment choose the binary rather than this file"
                )));
            }

            let real = std::fs::canonicalize(path).map_err(|e| {
                Error::Refused(format!("allowlist entry {entry:?} cannot be resolved: {e}"))
            })?;
            resolved.push(real);
        }

        Ok(Self { entries: resolved })
    }

    /// The real path this program resolves to, if it is allowed.
    ///
    /// Compares canonical paths rather than device and inode, and that choice is load bearing
    /// rather than incidental. Do not "improve" it into an inode comparison.
    ///
    /// On the machine this was written on, `/usr/bin/echo` and `/usr/bin/sleep` resolve to
    /// `/usr/lib/cargo/bin/coreutils/echo` and `.../sleep`, which are two names for one inode:
    /// uutils ships a single multi-call binary hard linked under every utility name. Comparing
    /// inodes would make those two entries identical, so allowing `echo` would allow `sleep`,
    /// `rm` and everything else in it. The allowlist would look intact and mean nothing.
    ///
    /// Comparing paths keeps them distinct. The cost is that a hard link to an allowed binary
    /// under some other path is refused, which is the right way round: the allowlist names
    /// paths, so a path it does not name is not on it, whatever inode sits behind it.
    ///
    /// The caller spawns the returned path rather than the spelling it was asked for. That
    /// also matters for a multi-call binary, which decides what to be from `argv[0]`. Spawning
    /// the resolved path makes it behave as the file that was actually checked. Passing the
    /// requested spelling as `arg0` would undo that: a symlink named `rm` pointing at an
    /// allowed `echo` would pass the check and then behave as `rm`.
    pub fn resolve_program(&self, program: &str) -> Result<PathBuf> {
        let real = std::fs::canonicalize(program).map_err(|e| {
            Error::Refused(format!(
                "{program:?} cannot be resolved to a real file, so it cannot be checked \
                 against the allowlist: {e}"
            ))
        })?;

        if self.entries.contains(&real) {
            return Ok(real);
        }

        Err(Error::Refused(format!(
            "{program:?} resolves to {} which is not an allowed program, allowed are {:?}",
            real.display(),
            self.entries
        )))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory holding a real executable, plus a second one for the impostor.
    fn two_dirs() -> (tempfile::TempDir, tempfile::TempDir) {
        (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap())
    }

    fn write_program(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\ntrue\n").unwrap();
        p
    }

    #[test]
    fn an_absolute_entry_resolves_and_matches_itself() {
        let (d, _) = two_dirs();
        let real = write_program(d.path(), "tool");

        let list = Allowlist::resolve([real.display().to_string()]).unwrap();
        assert_eq!(list.resolve_program(real.to_str().unwrap()).unwrap(), real);
    }

    /// The bug. A bare name would have been handed to `Command::new`, which searches `$PATH`
    /// at exec time, so whatever the daemon inherited would have chosen the binary.
    #[test]
    fn a_bare_name_is_refused_at_load() {
        let e = Allowlist::resolve(["probetool".to_string()])
            .unwrap_err()
            .to_string();
        assert!(e.contains("not an absolute path"), "{e}");
        assert!(e.contains("$PATH"), "{e}");
    }

    /// And the other half of it. A relative entry with a slash resolves against whatever
    /// working directory the daemon was started from.
    #[test]
    fn a_relative_entry_with_a_slash_is_refused_at_load() {
        let e = Allowlist::resolve(["bin/probetool".to_string()])
            .unwrap_err()
            .to_string();
        assert!(e.contains("not an absolute path"), "{e}");
    }

    /// An entry naming nothing is refused rather than sitting there matching nothing, because
    /// a rule that can never fire is usually a typo in a rule that was meant to.
    #[test]
    fn an_entry_that_does_not_exist_is_refused_at_load() {
        let (d, _) = two_dirs();
        let missing = d.path().join("not-here");
        let e = Allowlist::resolve([missing.display().to_string()])
            .unwrap_err()
            .to_string();
        assert!(e.contains("cannot be resolved"), "{e}");
    }

    /// A different file with the same base name is not the allowed program, however the
    /// request spells it. This is the shape the `$PATH` search produced.
    #[test]
    fn a_different_file_with_the_same_name_is_refused() {
        let (allowed_dir, impostor_dir) = two_dirs();
        let allowed = write_program(allowed_dir.path(), "tool");
        let impostor = write_program(impostor_dir.path(), "tool");

        let list = Allowlist::resolve([allowed.display().to_string()]).unwrap();
        let e = list
            .resolve_program(impostor.to_str().unwrap())
            .unwrap_err()
            .to_string();
        assert!(e.contains("not an allowed program"), "{e}");
    }

    /// A symlink to an allowed program is allowed, because canonicalizing resolves it to the
    /// same real file. The allowlist is about which file runs, not about how it was spelled.
    #[test]
    fn a_symlink_to_an_allowed_program_resolves_to_it() {
        let (d, other) = two_dirs();
        let real = write_program(d.path(), "tool");
        let link = other.path().join("shortcut");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let list = Allowlist::resolve([real.display().to_string()]).unwrap();
        assert_eq!(list.resolve_program(link.to_str().unwrap()).unwrap(), real);
    }

    /// Two real coreutils on this machine, which are one inode under two names because uutils
    /// hard links a single multi-call binary under every utility name. This is the test that
    /// says why the comparison is on paths: switch it to device and inode and allowing `echo`
    /// silently allows `sleep`, `rm`, and everything else in that binary.
    ///
    /// Skips rather than fails where the layout is different, since it is asserting something
    /// about the host rather than about this crate.
    #[test]
    fn two_hard_linked_coreutils_stay_distinct_entries() {
        let (echo, sleep) = ("/usr/bin/echo", "/usr/bin/sleep");
        let (Ok(re), Ok(rs)) = (std::fs::canonicalize(echo), std::fs::canonicalize(sleep)) else {
            return;
        };
        let same_inode = {
            use std::os::unix::fs::MetadataExt;
            match (std::fs::metadata(&re), std::fs::metadata(&rs)) {
                (Ok(a), Ok(b)) => a.ino() == b.ino() && a.dev() == b.dev(),
                _ => false,
            }
        };
        if !same_inode {
            return;
        }

        let list = Allowlist::resolve([echo.to_string()]).unwrap();
        assert!(
            list.resolve_program(sleep).is_err(),
            "{sleep} shares an inode with {echo} and was allowed by an entry naming only {echo}"
        );
    }

    /// A hard link is refused, and that is the deliberate half. It is the same inode, so an
    /// inode comparison would allow it. The allowlist names paths, and this path is not named.
    #[test]
    fn a_hard_link_to_an_allowed_program_is_refused() {
        let (d, other) = two_dirs();
        let real = write_program(d.path(), "tool");
        let hard = other.path().join("copy");
        std::fs::hard_link(&real, &hard).unwrap();

        let list = Allowlist::resolve([real.display().to_string()]).unwrap();
        assert!(list.resolve_program(hard.to_str().unwrap()).is_err());
    }
}
