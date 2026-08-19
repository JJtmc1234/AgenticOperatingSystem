//! The boundary. Every path an agent names is resolved inside a root, or refused.
//!
//! This is the load bearing part of the whole capability server. Everything above it, the
//! tiers, the policy and the plans, decides whether a call is allowed to happen. None of that
//! matters if the call can then name `../../../.ssh/id_rsa` and be handed it.
//!
//! Three ways out of a directory, and all three have to be closed:
//!
//! - an absolute path, which ignores the root entirely
//! - `..`, which walks up out of it
//! - a symlink inside the root pointing outside it
//!
//! The first two are text and can be refused by looking. The third cannot. A path made
//! entirely of innocent looking components can still land anywhere on the disk, and the only
//! way to know is to ask the kernel where it actually goes. So the check is done on the real
//! path after resolution, not on the string that was asked for.
//!
//! There is one further trap. A file that does not exist yet cannot be resolved, and refusing
//! to create files would make the server useless. So for a path that does not exist, the
//! parent is resolved instead, which does exist, and the final component is checked for being
//! a plain name.

use std::path::{Component, Path, PathBuf};

use crate::{Error, Result};

/// A directory an agent is allowed to work inside.
#[derive(Debug, Clone)]
pub struct Root {
    real: PathBuf,
}

impl Root {
    /// Opens a root. The directory must already exist, because a root that is created on
    /// demand is a root that a typo silently relocates.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let real = std::fs::canonicalize(path).map_err(|e| {
            Error::Refused(format!("root {} cannot be opened: {e}", path.display()))
        })?;
        if !real.is_dir() {
            return Err(Error::Refused(format!(
                "root {} is not a directory",
                real.display()
            )));
        }
        Ok(Self { real })
    }

    pub fn path(&self) -> &Path {
        &self.real
    }

    /// Resolves a path an agent asked for, for reading something that must already exist.
    pub fn existing(&self, asked: &str) -> Result<PathBuf> {
        let joined = self.join(asked)?;
        let real =
            std::fs::canonicalize(&joined).map_err(|e| Error::Refused(format!("{asked}: {e}")))?;
        self.contained(&real, asked)?;
        Ok(real)
    }

    /// Resolves a path for creating or replacing something, which may not exist yet.
    ///
    /// The parent has to exist and has to be inside the root. Checking the parent rather than
    /// the file is what makes creating a file possible at all, since a path that is not there
    /// cannot be resolved and an unresolvable path cannot be proven safe.
    pub fn for_writing(&self, asked: &str) -> Result<PathBuf> {
        let joined = self.join(asked)?;

        // Anything already at that name, so it can be checked properly.
        //
        // `symlink_metadata` rather than `exists`, because `exists` follows the link and so
        // answers about the target rather than about the name. A link whose target does not
        // exist yet therefore reported false, took the parent branch below, and was resolved
        // as though it were an ordinary new file. The parent is inside the root, so that
        // passed, and the write then followed the link to wherever it pointed. See bug 8.
        //
        // Taking this branch means `existing` runs `canonicalize`, which fails on a dangling
        // link, so it is refused. Refusing a link into empty space inside the root is a
        // narrowing, and the right one: a path that cannot be resolved cannot be proven safe.
        if std::fs::symlink_metadata(&joined).is_ok() {
            return self.existing(asked);
        }

        let parent = joined
            .parent()
            .ok_or_else(|| Error::Refused(format!("{asked} has no parent directory")))?;
        let name = joined
            .file_name()
            .ok_or_else(|| Error::Refused(format!("{asked} does not name a file")))?;

        let real_parent = std::fs::canonicalize(parent).map_err(|e| {
            Error::Refused(format!("the directory for {asked} does not exist: {e}"))
        })?;
        self.contained(&real_parent, asked)?;

        Ok(real_parent.join(name))
    }

    /// The part of a resolved path an agent should be shown.
    ///
    /// Relative to the root, because the absolute path leaks the layout of the machine and is
    /// no use to the agent anyway.
    pub fn shown(&self, real: &Path) -> String {
        real.strip_prefix(&self.real)
            .unwrap_or(real)
            .display()
            .to_string()
    }

    /// Joins without resolving, refusing the two textual escapes.
    fn join(&self, asked: &str) -> Result<PathBuf> {
        if asked.is_empty() {
            return Ok(self.real.clone());
        }
        let p = Path::new(asked);

        // Refused by looking rather than by resolving, so the reason given names the actual
        // problem instead of some downstream failure.
        for c in p.components() {
            match c {
                Component::Normal(_) | Component::CurDir => {}
                Component::ParentDir => {
                    return Err(Error::Refused(format!("{asked} walks out of the root")));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(Error::Refused(format!(
                        "{asked} is an absolute path, and paths are relative to the root"
                    )));
                }
            }
        }
        Ok(self.real.join(p))
    }

    /// The check that catches a symlink, which no amount of reading the string can.
    fn contained(&self, real: &Path, asked: &str) -> Result<()> {
        if real == self.real || real.starts_with(&self.real) {
            return Ok(());
        }
        Err(Error::Refused(format!(
            "{asked} resolves outside the root, so it is refused. A link inside the root can \
             still point out of it, which is why the check is on where a path actually goes \
             rather than on how it is spelled."
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> (tempfile::TempDir, Root) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("notes")).unwrap();
        std::fs::write(dir.path().join("notes/a.txt"), "hello").unwrap();
        let root = Root::open(dir.path()).unwrap();
        (dir, root)
    }

    #[test]
    fn an_ordinary_path_resolves() {
        let (_d, r) = root();
        let p = r.existing("notes/a.txt").unwrap();
        assert!(p.ends_with("notes/a.txt"));
        assert_eq!(r.shown(&p), "notes/a.txt");
    }

    #[test]
    fn dot_slash_is_harmless() {
        let (_d, r) = root();
        assert!(r.existing("./notes/./a.txt").is_ok());
    }

    /// The obvious escape, refused by reading the string.
    #[test]
    fn walking_up_is_refused() {
        let (_d, r) = root();
        for bad in ["../secret", "notes/../../secret", "..", "../"] {
            let e = r.existing(bad).unwrap_err().to_string();
            assert!(e.contains("walks out of the root"), "{bad}: {e}");
        }
    }

    #[test]
    fn an_absolute_path_is_refused() {
        let (_d, r) = root();
        for bad in ["/etc/passwd", "/", "/home"] {
            let e = r.existing(bad).unwrap_err().to_string();
            assert!(e.contains("absolute"), "{bad}: {e}");
        }
    }

    /// The one that string checking cannot catch. Every component here is innocent and the
    /// path still lands outside the root.
    #[test]
    fn a_symlink_pointing_out_of_the_root_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "not yours").unwrap();

        std::os::unix::fs::symlink(outside.path().join("secret"), dir.path().join("innocent"))
            .unwrap();

        let r = Root::open(dir.path()).unwrap();
        let e = r.existing("innocent").unwrap_err().to_string();
        assert!(e.contains("resolves outside the root"), "{e}");
    }

    /// A link inside the root is fine. Refusing every link would break ordinary directories.
    #[test]
    fn a_symlink_staying_inside_the_root_is_allowed() {
        let (d, r) = root();
        std::os::unix::fs::symlink(d.path().join("notes/a.txt"), d.path().join("shortcut"))
            .unwrap();
        assert!(r.existing("shortcut").is_ok());
    }

    /// Creating a file has to work, which means a path that does not exist yet has to be
    /// allowed through, which means the parent is what gets checked.
    #[test]
    fn a_new_file_inside_the_root_is_allowed() {
        let (_d, r) = root();
        let p = r.for_writing("notes/new.txt").unwrap();
        assert!(p.ends_with("notes/new.txt"));
        assert!(!p.exists(), "resolving must not create anything");
    }

    /// And the same escapes must still be closed on the writing path, which is the one that
    /// would be easy to leave open because it cannot resolve the file itself.
    #[test]
    fn a_new_file_outside_the_root_is_still_refused() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("elsewhere")).unwrap();
        let r = Root::open(dir.path()).unwrap();

        assert!(r.for_writing("../escaped.txt").is_err());
        assert!(r.for_writing("/tmp/escaped.txt").is_err());

        let e = r
            .for_writing("elsewhere/escaped.txt")
            .unwrap_err()
            .to_string();
        assert!(e.contains("resolves outside the root"), "{e}");
    }

    /// The hole the test above did not cover. That one links to a directory which already
    /// exists, so `exists()` is true and the checked path is taken. A link whose target does
    /// not exist yet reports false from `exists()`, because `exists()` follows the link, so it
    /// took the parent branch instead. The parent is the root, the root is inside itself, and
    /// the write then followed the link to wherever it pointed.
    ///
    /// Real shapes this takes: a checkout carrying `config -> ../../.ssh/authorized_keys`, or
    /// a stale `latest -> builds/2026-08-18/out.log` after the build directory was cleaned.
    #[test]
    fn a_dangling_symlink_pointing_outside_the_root_is_refused() {
        let (d, r) = root();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("not-there-yet.txt");
        std::os::unix::fs::symlink(&target, d.path().join("innocent")).unwrap();

        assert!(!target.exists(), "the target must not exist for this test");

        let outcome = r.for_writing("innocent");
        assert!(
            outcome.is_err(),
            "a write through a dangling link resolved to {outcome:?}, which is outside the root"
        );

        // And nothing was created on the way to deciding that.
        assert!(!target.exists(), "resolving must not create anything");
    }

    /// A dangling link that stays inside the root is refused too, and that is a deliberate
    /// narrowing rather than an oversight. The target does not exist, so it cannot be
    /// canonicalized, and a path that cannot be resolved cannot be proven safe. Refusing
    /// something harmless is the cost of never guessing about something that is not.
    #[test]
    fn a_dangling_symlink_pointing_inside_the_root_is_refused_too() {
        let (d, r) = root();
        std::os::unix::fs::symlink(d.path().join("notes/later.txt"), d.path().join("pending"))
            .unwrap();

        assert!(r.for_writing("pending").is_err());
    }

    #[test]
    fn writing_into_a_directory_that_does_not_exist_says_so() {
        let (_d, r) = root();
        let e = r.for_writing("nope/new.txt").unwrap_err().to_string();
        assert!(e.contains("does not exist"), "{e}");
    }

    #[test]
    fn an_empty_path_is_the_root_itself() {
        let (_d, r) = root();
        assert_eq!(r.existing("").unwrap(), *r.path());
    }

    /// A root that does not exist is a mistake worth failing on, not a directory to create.
    #[test]
    fn a_missing_root_is_refused() {
        assert!(Root::open("/definitely/not/here").is_err());
    }

    #[test]
    fn a_file_as_a_root_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("f");
        std::fs::write(&f, "x").unwrap();
        assert!(Root::open(&f).is_err());
    }
}
