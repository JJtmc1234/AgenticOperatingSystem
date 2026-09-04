//! Checking that the memory system's indexes are true.
//!
//! Ten agents remember things between conversations, and none of them can carry all of it in a
//! prompt. So the design is indexes: `shared/INDEX.md` lists every shared file, and each
//! `MEMORY.md` opens with a table of its own sections. An agent reads the rows that match the
//! work in front of it, not the folder.
//!
//! That only works while the indexes are true, and an index is exactly the kind of thing that
//! stops being true quietly. A file added without its row is a file no agent will ever find. A
//! row left behind after its file moves sends an agent to read something that is not there, and
//! an agent that follows a dead row once learns that the index is unreliable, which costs more
//! than the missing row did.
//!
//! `memory-system/README.md` said these were "Checked, both directions" for a month before
//! anything checked them. This is the thing that makes the sentence true.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

mod parse;
pub use parse::{index_rows, section_rows, sections_of};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} could not be read: {1}")]
    Unreadable(PathBuf, std::io::Error),
    #[error("{0} is not a folder this can check: {1}")]
    NotAMemorySystem(PathBuf, String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One thing wrong with an index.
///
/// Kept as data rather than printed where it is found, so a run reports everything at once. A
/// checker that stops at the first fault makes somebody run it five times to fix five rows.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fault {
    /// A file exists and no row mentions it. Nobody will find it.
    Unindexed { index: String, target: String },
    /// A row points at something that is not there. An agent following it reads nothing and
    /// learns the index cannot be trusted.
    Dangling { index: String, target: String },
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fault::Unindexed { index, target } => write!(
                f,
                "{target} has no row in {index}, so no agent will ever find it. Add the row in \
                 the same turn you added the file"
            ),
            Fault::Dangling { index, target } => write!(
                f,
                "{index} has a row for {target}, which does not exist. An agent that follows a \
                 dead row learns the index is unreliable"
            ),
        }
    }
}

/// Everything wrong with a memory system, in a fixed order.
pub fn check(root: &Path) -> Result<Vec<Fault>> {
    if !root.join("shared").is_dir() {
        return Err(Error::NotAMemorySystem(
            root.to_path_buf(),
            "there is no shared/ in it".into(),
        ));
    }
    let mut faults = check_shared(root)?;
    faults.extend(check_agents(root)?);
    faults.sort();
    Ok(faults)
}

/// `shared/INDEX.md` against the files in `shared/`, both directions.
fn check_shared(root: &Path) -> Result<Vec<Fault>> {
    let shared = root.join("shared");
    let index_path = shared.join("INDEX.md");
    let text = read(&index_path)?;

    let indexed: BTreeSet<String> = index_rows(&text).into_iter().collect();
    let present: BTreeSet<String> = files_under(&shared)?
        .into_iter()
        .map(|p| relative(&shared, &p))
        .collect();

    let index = "shared/INDEX.md".to_string();
    let mut faults = Vec::new();
    for file in present.difference(&indexed) {
        faults.push(Fault::Unindexed {
            index: index.clone(),
            target: format!("shared/{file}"),
        });
    }
    for row in indexed.difference(&present) {
        faults.push(Fault::Dangling {
            index: index.clone(),
            target: format!("shared/{row}"),
        });
    }
    Ok(faults)
}

/// Every `MEMORY.md` against its own opening table, both directions.
fn check_agents(root: &Path) -> Result<Vec<Fault>> {
    let mut faults = Vec::new();
    for path in files_under(&root.join("agents"))? {
        if path.file_name().and_then(|n| n.to_str()) != Some("MEMORY.md") {
            continue;
        }
        let text = read(&path)?;
        let where_it_is = relative(root, &path);

        let listed: BTreeSet<String> = section_rows(&text).into_iter().collect();
        let real: BTreeSet<String> = sections_of(&text).into_iter().collect();

        for section in real.difference(&listed) {
            faults.push(Fault::Unindexed {
                index: where_it_is.clone(),
                target: format!("the section {section:?}"),
            });
        }
        for row in listed.difference(&real) {
            faults.push(Fault::Dangling {
                index: where_it_is.clone(),
                target: format!("the section {row:?}"),
            });
        }
    }
    Ok(faults)
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| Error::Unreadable(path.to_path_buf(), e))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Every file under a folder, recursively, in a stable order.
fn files_under(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    if !dir.is_dir() {
        return Ok(found);
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| Error::Unreadable(dir.to_path_buf(), e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            found.extend(files_under(&path)?);
        } else {
            found.push(path);
        }
    }
    Ok(found)
}
