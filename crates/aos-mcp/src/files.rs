//! Carrying out the file capabilities, once something else has decided they are allowed.
//!
//! Every function here takes an already resolved path. That is deliberate: resolving is the
//! security boundary and it lives in `root.rs`, so nothing in this file takes a string from
//! an agent and turns it into a path. If it did, the boundary would have two implementations
//! and one of them would eventually be wrong.

use std::io::Read;
use std::path::Path;

use crate::{Error, Result, Root};

/// How much of a file to hand back at once.
///
/// A capability server that streams a two gigabyte log into a context window has not helped
/// anybody. Truncation says so rather than quietly cutting.
const MAX_READ: usize = 200_000;

/// How much is actually read off the disk.
///
/// `MAX_READ` bounds the reply. This bounds the read, and they are not the same thing: the
/// old code read the whole file and only then truncated the string, so a multi gigabyte log
/// sitting inside the root took the process down with the OOM killer, which drops the whole
/// MCP session rather than failing the one call. See bug 7.
///
/// The four extra bytes are one utf8 character, which is what makes it possible to tell a
/// cut this code made from a file that is genuinely not utf8.
const READ_CAP: u64 = MAX_READ as u64 + 4;

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
    // The size is asked for separately so the reply can say how much was left, without the
    // whole file having to be read to find out.
    let size = std::fs::metadata(path)?.len();

    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(READ_CAP)
        .read_to_end(&mut bytes)?;
    let short_read = (bytes.len() as u64) < size;

    // Told, not guessed at. An agent handed mangled text with no warning will reason about
    // the mangling as though it were content.
    let (text, note) = match std::str::from_utf8(&bytes) {
        Ok(t) => (t.to_string(), None),
        // `error_len` is `None` only for input that ran out part way through a character. If
        // this code stopped reading early then that is where the character went, so it is a
        // cut this code made rather than a file that is broken, and saying otherwise would
        // have an agent reasoning about damage that is not there.
        Err(e) if short_read && e.error_len().is_none() => {
            let valid = e.valid_up_to();
            (
                std::str::from_utf8(&bytes[..valid])
                    .unwrap_or_default()
                    .to_string(),
                None,
            )
        }
        Err(_) => (
            String::from_utf8_lossy(&bytes).into_owned(),
            Some("this file is not valid utf8, so some characters were replaced"),
        ),
    };

    let mut out = if text.len() > MAX_READ || short_read {
        let mut cut = text.len().min(MAX_READ);
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        format!(
            "{}\n\n... truncated, {} bytes of {} shown",
            &text[..cut],
            cut,
            size
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
    let needle = contains.to_lowercase();
    let mut hits = Vec::new();
    walk(root.path(), &needle, limit, &mut hits)?;

    if hits.is_empty() {
        return Ok(format!("nothing under the root matches {contains:?}"));
    }
    let shown: Vec<String> = hits.iter().map(|p| root.shown(p)).collect();
    Ok(shown.join("\n"))
}

fn walk(dir: &Path, needle: &str, limit: usize, hits: &mut Vec<std::path::PathBuf>) -> Result<()> {
    if hits.len() >= limit {
        return Ok(());
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        // A directory that cannot be read is skipped rather than fatal. One unreadable folder
        // must not make a search over a whole tree fail.
        Err(_) => return Ok(()),
    };

    for e in entries.filter_map(|e| e.ok()) {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_lowercase();
        if name.contains(needle) {
            hits.push(path.clone());
            if hits.len() >= limit {
                return Ok(());
            }
        }
        // Only real directories are followed. A symlinked directory could point anywhere,
        // including at a parent of itself, and following those is how a search never returns.
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            walk(&path, needle, limit, hits)?;
        }
    }
    Ok(())
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

    /// Peak resident memory of this process, in kB, from `/proc/self/status`.
    fn peak_rss_kb() -> u64 {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        status
            .lines()
            .find_map(|l| l.strip_prefix("VmHWM:"))
            .and_then(|v| v.split_whitespace().next()?.parse().ok())
            .unwrap_or(0)
    }

    /// The bug. `std::fs::read` pulled the whole file in and `bytes.clone()` made a second
    /// copy for the utf8 check, so peak memory was twice the file size and `MAX_READ` bounded
    /// only the reply. A large log inside the root took the process down with the OOM killer,
    /// which drops the whole MCP session rather than failing the one call.
    ///
    /// Measures rather than asserts on the implementation, because the property that matters
    /// is what the process actually does to the machine.
    #[test]
    fn reading_a_large_file_does_not_pull_it_all_into_memory() {
        let (d, r) = root();
        let big = d.path().join("big.log");

        // 80 MB, which is 400 times MAX_READ and small enough not to be unkind to a test run.
        let chunk = "x".repeat(1024 * 1024);
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&big).unwrap();
            for _ in 0..80 {
                f.write_all(chunk.as_bytes()).unwrap();
            }
        }

        let before = peak_rss_kb();
        let out = read_file(&r, &r.existing("big.log").unwrap()).unwrap();
        let after = peak_rss_kb();

        assert!(out.len() < MAX_READ + 500, "reply was {} bytes", out.len());
        assert!(out.contains("truncated"), "a cut reply has to say so");

        // Against the old code this grew by roughly the file size. A few MB of slack, because
        // the decoded string and the reply are both real and both bounded by MAX_READ.
        let grew_kb = after.saturating_sub(before);
        assert!(
            grew_kb < 8 * 1024,
            "peak memory grew by {grew_kb} kB reading an 80 MB file, so it is still being \
             read whole"
        );
    }

    /// A cut can land in the middle of a character, and that is this code's doing rather than
    /// the file's. Saying the file is not utf8 would have an agent reasoning about damage that
    /// is not there.
    #[test]
    fn a_cut_through_a_character_is_not_reported_as_a_broken_file() {
        let (d, r) = root();
        let p = d.path().join("wide.txt");

        // Three byte characters, so the cap lands mid character rather than between two.
        let text = "\u{4e2d}".repeat(MAX_READ);
        std::fs::write(&p, &text).unwrap();

        let out = read_file(&r, &r.existing("wide.txt").unwrap()).unwrap();
        assert!(out.contains("truncated"), "{}", &out[out.len() - 80..]);
        assert!(
            !out.contains("not valid utf8"),
            "our own cut was reported as the file being broken"
        );
    }

    /// And a file that really is not utf8 still says so, which is the behaviour the case above
    /// must not have swallowed.
    #[test]
    fn a_genuinely_invalid_file_is_still_reported() {
        let (d, r) = root();
        std::fs::write(d.path().join("raw.bin"), [0xff, 0xfe, 0x00, 0x41]).unwrap();

        let out = read_file(&r, &r.existing("raw.bin").unwrap()).unwrap();
        assert!(out.contains("not valid utf8"), "{out}");
    }

    /// The truncation note reports the size of the file, not the size of the part that was
    /// read, or it would always claim the file is exactly as long as the cap.
    #[test]
    fn the_truncation_note_gives_the_real_file_size() {
        let (d, r) = root();
        let size = MAX_READ + 123_456;
        std::fs::write(d.path().join("big2.txt"), "y".repeat(size)).unwrap();

        let out = read_file(&r, &r.existing("big2.txt").unwrap()).unwrap();
        assert!(
            out.contains(&format!("of {size} shown")),
            "{}",
            &out[out.len() - 80..]
        );
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
