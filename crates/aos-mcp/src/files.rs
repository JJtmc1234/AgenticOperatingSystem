//! Carrying out the file capabilities, once something else has decided they are allowed.
//!
//! Every function here takes an already resolved path. That is deliberate: resolving is the
//! security boundary and it lives in `root.rs`, so nothing in this file takes a string from
//! an agent and turns it into a path. If it did, the boundary would have two implementations
//! and one of them would eventually be wrong.

use std::path::Path;

use crate::{Error, Result, Root};

/// How much of a file to hand back at once.
///
/// A capability server that streams a two gigabyte log into a context window has not helped
/// anybody. Truncation says so rather than quietly cutting.
const MAX_READ: usize = 200_000;

/// How many entries to list before giving up on being useful.
const MAX_ENTRIES: usize = 1_000;

pub fn list_dir(root: &Root, dir: &Path) -> Result<String> {
    if !dir.is_dir() {
        return Err(Error::Refused(format!(
            "{} is not a directory",
            root.shown(dir)
        )));
    }

    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    let total = entries.len();
    for e in entries.iter().take(MAX_ENTRIES) {
        let name = e.file_name().to_string_lossy().into_owned();
        let meta = e.metadata().ok();
        let kind = match &meta {
            Some(m) if m.is_dir() => "dir ",
            Some(m) if m.file_type().is_symlink() => "link",
            Some(_) => "file",
            None => "?   ",
        };
        let size = meta.filter(|m| m.is_file()).map(|m| m.len());
        match size {
            Some(n) => out.push(format!("{kind}  {n:>10}  {name}")),
            None => out.push(format!("{kind}  {:>10}  {name}", "")),
        }
    }
    if total > MAX_ENTRIES {
        out.push(format!("... and {} more", total - MAX_ENTRIES));
    }
    if out.is_empty() {
        out.push("(empty)".into());
    }
    Ok(out.join("\n"))
}

pub fn read_file(root: &Root, path: &Path) -> Result<String> {
    if path.is_dir() {
        return Err(Error::Refused(format!(
            "{} is a directory, so list it rather than reading it",
            root.shown(path)
        )));
    }
    let bytes = std::fs::read(path)?;

    // Told, not guessed at. An agent handed mangled text with no warning will reason about
    // the mangling as though it were content.
    let (text, note) = match String::from_utf8(bytes.clone()) {
        Ok(t) => (t, None),
        Err(_) => (
            String::from_utf8_lossy(&bytes).into_owned(),
            Some("this file is not valid utf8, so some characters were replaced"),
        ),
    };

    let mut out = if text.len() > MAX_READ {
        let mut cut = MAX_READ;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        format!(
            "{}\n\n... truncated, {} bytes of {} shown",
            &text[..cut],
            cut,
            text.len()
        )
    } else {
        text
    };

    if let Some(n) = note {
        out.push_str(&format!("\n\n({n})"));
    }
    Ok(out)
}

pub fn find(root: &Root, contains: &str, limit: usize) -> Result<String> {
    if contains.is_empty() {
        return Err(Error::Refused(
            "searching for an empty string would match everything".into(),
        ));
    }
    // Refused rather than clamped up, because a caller who asked for nothing back has said
    // something contradictory and should hear about it. `walk` returns immediately on a limit
    // of zero, so the old answer to `limit: 0` was "nothing matches" for a search that never
    // read a single directory, which is a confident wrong answer rather than an empty one.
    // See bug 7.
    if limit == 0 {
        return Err(Error::Refused(
            "a limit of 0 asks for no results, so nothing would be searched. Ask for at least 1"
                .into(),
        ));
    }

    let needle = contains.to_lowercase();
    let mut hits = Vec::new();
    let complete = walk(root.path(), &needle, limit, &mut hits)?;

    if hits.is_empty() {
        return Ok(format!("nothing under the root matches {contains:?}"));
    }
    let mut shown: Vec<String> = hits.iter().map(|p| root.shown(p)).collect();

    // Said, not implied. `list_dir` appends "... and N more" and `read_file` appends
    // "... truncated", and this was the one capability of the three that cut silently. A model
    // that asked for every match and got the first 200 of 250 will act on those 200 as if they
    // were all of them.
    //
    // The walk order is named too, because it is not sorted and not anything the caller chose,
    // so "the first 200" means the first 200 the filesystem happened to hand over.
    if !complete {
        shown.push(format!(
            "\n... stopped at the limit of {limit}, and there are more. These are the first \
             {limit} in filesystem order, not the best or the newest. Search for something \
             narrower, or ask for a higher limit."
        ));
    }
    Ok(shown.join("\n"))
}

/// Walks the tree collecting matches. `true` means the whole tree was searched, `false` means
/// it stopped at the limit and there is more.
///
/// Reporting that is the point. Returning the hits alone cannot tell "these are all of them"
/// from "these are the first `limit` of an unknown number", and those call for different
/// actions by whoever is reading.
fn walk(
    dir: &Path,
    needle: &str,
    limit: usize,
    hits: &mut Vec<std::path::PathBuf>,
) -> Result<bool> {
    if hits.len() >= limit {
        return Ok(false);
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        // A directory that cannot be read is skipped rather than fatal. One unreadable folder
        // must not make a search over a whole tree fail.
        Err(_) => return Ok(true),
    };

    for e in entries.filter_map(|e| e.ok()) {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_lowercase();
        if name.contains(needle) {
            hits.push(path.clone());
            if hits.len() >= limit {
                return Ok(false);
            }
        }
        // Only real directories are followed. A symlinked directory could point anywhere,
        // including at a parent of itself, and following those is how a search never returns.
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) && !walk(&path, needle, limit, hits)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn write_file(root: &Root, path: &Path, text: &str) -> Result<String> {
    if path.is_dir() {
        return Err(Error::Refused(format!(
            "{} is a directory",
            root.shown(path)
        )));
    }
    let existed = path.exists();
    std::fs::write(path, text)?;
    Ok(format!(
        "{} {} ({} bytes)",
        if existed { "replaced" } else { "wrote" },
        root.shown(path),
        text.len()
    ))
}

pub fn make_dir(root: &Root, path: &Path) -> Result<String> {
    if path.is_dir() {
        return Ok(format!("{} already exists", root.shown(path)));
    }
    std::fs::create_dir_all(path)?;
    Ok(format!("made {}", root.shown(path)))
}

pub fn move_file(root: &Root, from: &Path, to: &Path) -> Result<String> {
    // Refusing to overwrite is the whole reason this is not a rename call. A move that
    // silently replaces the file at the destination loses data that nobody agreed to lose,
    // and the plan the user approved said move, not replace.
    if to.exists() {
        return Err(Error::Refused(format!(
            "{} already exists, and moving onto it would lose it",
            root.shown(to)
        )));
    }
    std::fs::rename(from, to)?;
    Ok(format!("moved {} to {}", root.shown(from), root.shown(to)))
}

pub fn delete_file(root: &Root, path: &Path) -> Result<String> {
    // Only files. A recursive delete is a different tier of mistake and needs its own
    // capability, its own plan and its own argument about whether it should exist at all.
    if path.is_dir() {
        return Err(Error::Refused(format!(
            "{} is a directory, and this only deletes files",
            root.shown(path)
        )));
    }
    std::fs::remove_file(path)?;
    Ok(format!("deleted {}", root.shown(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> (tempfile::TempDir, Root) {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("notes")).unwrap();
        std::fs::write(d.path().join("notes/a.txt"), "hello").unwrap();
        std::fs::write(d.path().join("b.txt"), "world").unwrap();
        let r = Root::open(d.path()).unwrap();
        (d, r)
    }

    #[test]
    fn listing_shows_kind_and_size() {
        let (_d, r) = root();
        let out = list_dir(&r, r.path()).unwrap();
        assert!(out.contains("notes"), "{out}");
        assert!(out.contains("b.txt"), "{out}");
        assert!(out.contains("dir"), "{out}");
    }

    #[test]
    fn reading_gives_the_text_back() {
        let (_d, r) = root();
        let p = r.existing("notes/a.txt").unwrap();
        assert_eq!(read_file(&r, &p).unwrap(), "hello");
    }

    /// An agent handed mangled text with no warning reasons about the mangling as content.
    #[test]
    fn a_binary_file_says_it_was_mangled() {
        let (d, r) = root();
        std::fs::write(d.path().join("bin"), [0xff, 0xfe, 0x00, 0x01]).unwrap();
        let p = r.existing("bin").unwrap();
        assert!(read_file(&r, &p).unwrap().contains("not valid utf8"));
    }

    #[test]
    fn a_huge_file_is_cut_and_says_so() {
        let (d, r) = root();
        std::fs::write(d.path().join("big"), "x".repeat(MAX_READ + 5_000)).unwrap();
        let p = r.existing("big").unwrap();
        let out = read_file(&r, &p).unwrap();
        assert!(out.contains("truncated"), "must not cut silently");
        assert!(out.len() < MAX_READ + 500);
    }

    /// The bug. `find` walked until it had `limit` hits and returned them with nothing saying
    /// the search was cut off, so a model that asked for every match and got the first 200 of
    /// 250 acts on those 200 as if they were all of them.
    ///
    /// `list_dir` appends "... and N more" and `read_file` appends "... truncated". `find` was
    /// the one capability of the three that cut silently.
    #[test]
    fn a_search_that_hit_the_limit_says_so() {
        let (d, r) = root();
        for i in 0..25 {
            std::fs::write(d.path().join(format!("report{i:03}.txt")), "x").unwrap();
        }

        let out = find(&r, "report", 10).unwrap();
        assert_eq!(
            out.lines().filter(|l| l.contains("report")).count(),
            10,
            "the limit still holds"
        );
        assert!(out.contains("stopped at the limit"), "{out}");
        assert!(
            out.contains("filesystem order"),
            "the order is not the caller's and has to be named: {out}"
        );
    }

    /// And a search that saw the whole tree says nothing extra, or every answer would carry a
    /// warning and the warning would stop meaning anything.
    #[test]
    fn a_complete_search_is_not_marked_as_cut() {
        let (d, r) = root();
        std::fs::write(d.path().join("only-report.txt"), "x").unwrap();

        let out = find(&r, "report", 10).unwrap();
        assert!(!out.contains("stopped at the limit"), "{out}");
    }

    /// A limit reached inside a subdirectory has to stop the whole walk, not just that branch.
    /// Returning to the parent and carrying on would collect past the limit, and reporting the
    /// search as complete would be the original bug wearing a different hat.
    #[test]
    fn a_limit_hit_deep_in_the_tree_stops_everything() {
        let (d, r) = root();
        std::fs::create_dir_all(d.path().join("deep/deeper")).unwrap();
        for i in 0..20 {
            std::fs::write(d.path().join(format!("deep/deeper/report{i:03}.txt")), "x").unwrap();
        }
        std::fs::write(d.path().join("report-at-the-top.txt"), "x").unwrap();

        let out = find(&r, "report", 5).unwrap();
        assert_eq!(out.lines().filter(|l| l.contains("report")).count(), 5);
        assert!(out.contains("stopped at the limit"), "{out}");
    }

    /// `limit: 0` used to answer "nothing matches" for a search that never read a directory,
    /// which is a confident wrong answer rather than an empty one. The schema permits it, so
    /// it has to be handled rather than assumed away.
    #[test]
    fn a_limit_of_zero_is_refused_rather_than_answered_with_nothing() {
        let (_d, r) = root();
        let e = match find(&r, "a.txt", 0) {
            Err(e) => e.to_string(),
            Ok(out) => panic!("a search that never ran must not answer: {out}"),
        };
        assert!(e.contains("at least 1"), "{e}");
    }

    #[test]
    fn find_matches_on_part_of_a_name() {
        let (_d, r) = root();
        let out = find(&r, "a.tx", 10).unwrap();
        assert_eq!(out, "notes/a.txt");
    }

    #[test]
    fn find_with_nothing_to_look_for_is_refused() {
        let (_d, r) = root();
        assert!(find(&r, "", 10).is_err());
    }

    #[test]
    fn find_says_so_rather_than_returning_nothing() {
        let (_d, r) = root();
        assert!(find(&r, "zzzz", 10).unwrap().contains("nothing"));
    }

    #[test]
    fn writing_creates_and_then_replaces() {
        let (_d, r) = root();
        let p = r.for_writing("new.txt").unwrap();
        assert!(write_file(&r, &p, "one").unwrap().starts_with("wrote"));
        let p = r.for_writing("new.txt").unwrap();
        assert!(write_file(&r, &p, "two").unwrap().starts_with("replaced"));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
    }

    /// The plan said move, not replace. Losing the destination is not what was agreed to.
    #[test]
    fn moving_onto_an_existing_file_is_refused() {
        let (_d, r) = root();
        let from = r.existing("notes/a.txt").unwrap();
        let to = r.existing("b.txt").unwrap();
        let e = move_file(&r, &from, &to).unwrap_err().to_string();
        assert!(e.contains("already exists"), "{e}");
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "world", "untouched");
    }

    #[test]
    fn moving_to_a_free_name_works() {
        let (_d, r) = root();
        let from = r.existing("notes/a.txt").unwrap();
        let to = r.for_writing("notes/renamed.txt").unwrap();
        assert!(move_file(&r, &from, &to).is_ok());
        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "hello");
    }

    /// A recursive delete is a different tier of mistake and needs its own argument about
    /// whether it should exist. It is not something to get for free from a file delete.
    #[test]
    fn deleting_a_directory_is_refused() {
        let (_d, r) = root();
        let p = r.existing("notes").unwrap();
        let e = delete_file(&r, &p).unwrap_err().to_string();
        assert!(e.contains("only deletes files"), "{e}");
        assert!(p.exists());
    }

    #[test]
    fn deleting_a_file_works() {
        let (_d, r) = root();
        let p = r.existing("b.txt").unwrap();
        assert!(delete_file(&r, &p).is_ok());
        assert!(!p.exists());
    }
}
