//! What an agent may read, the narrower place it may change, and what is off limits in both.
//!
//! `root.rs` answers one question: is this path inside that directory. This answers the question
//! above it, which is that reading and changing are not the same grant and should never have been
//! the same directory.
//!
//! A worker is given a project to read, because work that cannot see the code around it is work
//! done blind. It is given one task workspace to change, because a worker that can write anywhere
//! it can read is a worker whose mistake is unbounded. Those are different sizes on purpose, and
//! the lead granting the second one is the point at which somebody decided.
//!
//! **No write scope means nothing may be changed.** Not "the whole read root", which is what the
//! server used to do when there was only one directory. A capability nobody granted is a
//! capability that is not held, and the alternative default is one where forgetting a flag is
//! indistinguishable from deciding to allow it.
//!
//! **The write scope has to be inside the read scope.** An agent writing where it cannot read is
//! an agent that cannot check its own work, and allowing the two to be unrelated would let a
//! grant reach a path the read root was chosen to exclude.
//!
//! **Some names are refused everywhere, whatever the roots say.** A root is about where a path
//! goes. This is about what it is. A private key sitting inside a project directory is inside the
//! root by every check `root.rs` makes, and handing it over is still the worst thing this server
//! could do. The list is names rather than contents, because a rule that has to open the file to
//! decide has already read it.

use std::path::{Path, PathBuf};

use crate::{Error, Result, Root};

/// Names that are refused wherever they appear, as whole path components.
///
/// Directories mostly, because refusing the directory refuses everything under it without having
/// to guess at the names inside. Kept short and specific: a list that refuses too much makes
/// somebody widen it in a hurry, and a widened list is how the narrow ones get removed.
const SECRET_NAMES: &[&str] = &[
    ".ssh",
    ".aws",
    ".gnupg",
    ".env",
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
    "credentials",
    "secrets",
    // The agent's own state and other agents' memory. Nothing doing one task has a reason to
    // read either, and the second is somebody else's.
    ".carl",
    ".claude",
];

/// Endings that are refused wherever they appear.
const SECRET_ENDINGS: &[&str] = &[".pem", ".key", ".p12", ".pfx", ".keystore"];

/// Beginnings that are refused, because what follows them does not make the file less of a key.
const KEY_STEMS: &[&str] = &["id_rsa", "id_ecdsa", "id_ed25519", "id_dsa"];

/// What one agent may do to files, for the length of one task.
#[derive(Debug, Clone)]
pub struct Scope {
    read: Root,
    /// Where changes may land. `None` means none may.
    write: Option<Root>,
}

impl Scope {
    /// A scope that may look and may not touch.
    pub fn reading(read: Root) -> Self {
        Self { read, write: None }
    }

    /// The same, plus one directory inside it where changes are allowed.
    ///
    /// The directory has to exist. Creating it here would mean a typo silently becomes a new
    /// write scope somewhere nobody meant, which is the failure this whole module is about.
    pub fn granting(read: Root, write: impl AsRef<Path>) -> Result<Self> {
        let write = Root::open(write.as_ref())?;
        if !write.path().starts_with(read.path()) {
            return Err(Error::Refused(format!(
                "the write scope {} is not inside what may be read, {}. An agent that can change \
                 what it cannot read cannot check its own work.",
                write.path().display(),
                read.path().display()
            )));
        }
        Ok(Self {
            read,
            write: Some(write),
        })
    }

    /// What may be read. Also what paths are named relative to, so an agent sees one namespace
    /// rather than having to know which of its two roots a path belongs to.
    pub fn read(&self) -> &Root {
        &self.read
    }

    /// Where changes may land, when anywhere may.
    pub fn write(&self) -> Option<&Root> {
        self.write.as_ref()
    }

    /// One line for the person starting the server, so a scope nobody meant is visible at once.
    pub fn describe(&self) -> String {
        match &self.write {
            Some(w) => format!(
                "reading {}, changing only {}",
                self.read.path().display(),
                w.path().display()
            ),
            None => format!("reading {}, changing nothing", self.read.path().display()),
        }
    }

    /// Resolves a path for reading something that is already there.
    pub fn to_read(&self, asked: &str) -> Result<PathBuf> {
        let real = self.read.existing(asked)?;
        refuse_secrets(asked, &real)?;
        Ok(real)
    }

    /// Resolves a path for creating or replacing something.
    pub fn to_change(&self, asked: &str) -> Result<PathBuf> {
        let real = self.read.for_writing(asked)?;
        self.changeable(&real, asked)?;
        refuse_secrets(asked, &real)?;
        Ok(real)
    }

    /// Resolves a path for removing something, which has to be there to be removed.
    ///
    /// Separate from `to_change` only because it must exist. It is the same grant: deleting a
    /// file is changing it in the way that cannot be undone, so it is checked against the write
    /// scope and not against the read one.
    pub fn to_remove(&self, asked: &str) -> Result<PathBuf> {
        let real = self.read.existing(asked)?;
        self.changeable(&real, asked)?;
        refuse_secrets(asked, &real)?;
        Ok(real)
    }

    fn changeable(&self, real: &Path, asked: &str) -> Result<()> {
        let Some(write) = &self.write else {
            return Err(Error::Refused(format!(
                "{asked} cannot be changed, because this agent was granted no write scope. \
                 Reading and changing are separate grants and only a lead hands out the second."
            )));
        };
        if real.starts_with(write.path()) {
            return Ok(());
        }
        Err(Error::Refused(format!(
            "{asked} is readable but not writable. Changes may only land under {}, which is the \
             workspace this task was given.",
            self.read.shown(write.path())
        )))
    }
}

/// Refuses a path whose name says it holds a secret, wherever it sits.
///
/// Both the name asked for and the name it resolved to, because a link called `notes.txt` that
/// lands on `id_rsa` is caught by the second, and a path naming `.ssh` that does not exist yet is
/// caught by the first.
fn refuse_secrets(asked: &str, real: &Path) -> Result<()> {
    for path in [Path::new(asked), real] {
        if let Some(bad) = secret_part(path) {
            return Err(Error::Refused(format!(
                "{asked} is refused because of {bad:?}. Nothing doing one task has a reason to \
                 read keys, credentials or another agent's memory, and this is checked on the \
                 name rather than the contents, because a rule that opens the file to decide has \
                 already read it."
            )));
        }
    }
    Ok(())
}

fn secret_part(path: &Path) -> Option<String> {
    for part in path.components() {
        let name = part.as_os_str().to_string_lossy().to_lowercase();
        let bad = SECRET_NAMES.contains(&name.as_str())
            // A key is still a key with something on the end of it, and id_rsa.pub sitting next
            // to id_rsa is the thing somebody scans a directory for.
            || KEY_STEMS.iter().any(|stem| name.starts_with(stem))
            || SECRET_ENDINGS.iter().any(|e| name.ends_with(e));
        if bad {
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project with one task workspace inside it, which is the shape a lead grants.
    fn project() -> (tempfile::TempDir, Scope) {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::create_dir_all(d.path().join("task")).unwrap();
        std::fs::write(d.path().join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(d.path().join("task/notes.md"), "so far").unwrap();

        let read = Root::open(d.path()).unwrap();
        let scope = Scope::granting(read, d.path().join("task")).unwrap();
        (d, scope)
    }

    /// Work that cannot see the code around it is work done blind, so the read scope is wide.
    #[test]
    fn anything_in_the_project_can_be_read() {
        let (_d, scope) = project();
        assert!(scope.to_read("src/main.rs").is_ok());
        assert!(scope.to_read("task/notes.md").is_ok());
        assert!(scope.to_read("").is_ok(), "the project itself");
    }

    /// And the changes land in one place, which is the whole reason the two are separate.
    #[test]
    fn changes_land_in_the_task_workspace() {
        let (_d, scope) = project();
        assert!(scope.to_change("task/result.txt").is_ok());
        assert!(scope.to_change("task/notes.md").is_ok(), "and replace one");
        assert!(scope.to_remove("task/notes.md").is_ok());
    }

    /// Readable is not writable, and the message has to say which of the two it was, because
    /// "refused" on its own sends somebody looking at the wrong gate.
    #[test]
    fn what_can_be_read_outside_the_workspace_still_cannot_be_changed() {
        let (_d, scope) = project();
        assert!(scope.to_read("src/main.rs").is_ok());

        let e = scope.to_change("src/main.rs").unwrap_err().to_string();
        assert!(e.contains("readable but not writable"), "{e}");
        assert!(e.contains("task"), "and says where changes may go: {e}");

        assert!(
            scope.to_change("new.txt").is_err(),
            "nor a new file beside it"
        );
        assert!(scope.to_remove("src/main.rs").is_err());
    }

    /// A move takes the file away from where it was, so the source is a change and not a read.
    /// Checking only the destination would let an agent empty a directory it was given to read.
    #[test]
    fn a_file_cannot_be_moved_out_of_a_directory_that_is_only_readable() {
        let (_d, scope) = project();
        assert!(
            scope.to_remove("src/main.rs").is_err(),
            "the source of a move"
        );
        assert!(scope.to_change("task/main.rs").is_ok(), "the destination");
    }

    /// Forgetting a flag must not look like deciding to allow something.
    #[test]
    fn a_scope_nobody_granted_a_write_to_changes_nothing() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.txt"), "x").unwrap();
        let scope = Scope::reading(Root::open(d.path()).unwrap());

        assert!(scope.to_read("a.txt").is_ok());
        let e = scope.to_change("a.txt").unwrap_err().to_string();
        assert!(e.contains("no write scope"), "{e}");
        assert!(scope.to_remove("a.txt").is_err());
        assert!(
            scope.describe().contains("changing nothing"),
            "{}",
            scope.describe()
        );
    }

    /// An agent writing where it cannot read cannot check its own work, and a grant that reached
    /// outside would undo whatever the read root was narrowed for.
    #[test]
    fn a_write_scope_outside_the_read_scope_is_refused() {
        let inside = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let read = Root::open(inside.path()).unwrap();

        let e = Scope::granting(read, outside.path())
            .unwrap_err()
            .to_string();
        assert!(e.contains("not inside what may be read"), "{e}");
    }

    /// A write scope that does not exist yet is a typo, and creating it would make the typo
    /// into a new writable directory somewhere nobody meant.
    #[test]
    fn a_write_scope_that_is_not_there_is_refused_rather_than_created() {
        let d = tempfile::tempdir().unwrap();
        let read = Root::open(d.path()).unwrap();
        assert!(Scope::granting(read, d.path().join("nope")).is_err());
        assert!(!d.path().join("nope").exists(), "and nothing was made");
    }

    /// The point of the name list. Every one of these is inside the root by every check
    /// `root.rs` makes, and handing any of them over is still the worst thing this could do.
    #[test]
    fn a_secret_inside_the_project_is_refused_even_though_it_is_inside_the_root() {
        let d = tempfile::tempdir().unwrap();
        for at in [
            ".ssh/id_rsa",
            ".ssh/id_rsa.pub",
            ".aws/credentials",
            "deploy/server.pem",
            "config/.env",
            "secrets/token.txt",
            ".claude/settings.json",
            ".carl/memory/summary.md",
            "keys/id_ed25519",
            "certs/private.key",
        ] {
            let path = d.path().join(at);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "not yours").unwrap();
        }

        let read = Root::open(d.path()).unwrap();
        let scope = Scope::granting(read, d.path()).unwrap();

        for at in [
            ".ssh/id_rsa",
            ".ssh/id_rsa.pub",
            ".aws/credentials",
            "deploy/server.pem",
            "config/.env",
            "secrets/token.txt",
            ".claude/settings.json",
            ".carl/memory/summary.md",
            "keys/id_ed25519",
            "certs/private.key",
        ] {
            let e = scope.to_read(at).unwrap_err().to_string();
            assert!(e.contains("refused because of"), "{at} was allowed: {e}");
            assert!(scope.to_change(at).is_err(), "{at} could be written");
            assert!(scope.to_remove(at).is_err(), "{at} could be deleted");
        }
    }

    /// A link is the way round a name check, so the name that matters is the one it lands on.
    #[test]
    fn a_harmless_name_pointing_at_a_secret_is_refused() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join(".ssh")).unwrap();
        std::fs::write(d.path().join(".ssh/id_rsa"), "not yours").unwrap();
        std::os::unix::fs::symlink(d.path().join(".ssh/id_rsa"), d.path().join("notes.txt"))
            .unwrap();

        let scope = Scope::granting(Root::open(d.path()).unwrap(), d.path()).unwrap();
        let e = scope.to_read("notes.txt").unwrap_err().to_string();
        assert!(e.contains("refused because of"), "{e}");
    }

    /// And a name check that refused ordinary files would be widened by whoever hit it next.
    #[test]
    fn ordinary_files_are_not_mistaken_for_secrets() {
        let (_d, scope) = project();
        for at in ["src/main.rs", "task/notes.md"] {
            assert!(scope.to_read(at).is_ok(), "{at}");
        }
    }

    /// The escapes `root.rs` closes are still closed through a scope, on both resolvers, because
    /// a second way in that only the new code uses is the one nobody would think to test.
    #[test]
    fn the_ways_out_of_a_root_are_still_shut() {
        let (d, scope) = project();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), "not yours").unwrap();
        std::os::unix::fs::symlink(outside.path(), d.path().join("task/elsewhere")).unwrap();

        for bad in [
            "../secret",
            "task/../../secret",
            "/etc/passwd",
            "task/elsewhere/secret",
        ] {
            assert!(scope.to_read(bad).is_err(), "read {bad}");
            assert!(scope.to_change(bad).is_err(), "change {bad}");
            assert!(scope.to_remove(bad).is_err(), "remove {bad}");
        }
    }
}
