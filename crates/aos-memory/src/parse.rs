//! Reading the two index shapes out of markdown.
//!
//! Deliberately not a markdown parser. These are two known table shapes written by hand, and a
//! parser would accept a great deal that a person reading the folder would call wrong.
//!
//! Which table a row belongs to is decided by that table's own header, not by where it sits in
//! the file. Both documents have more than one table in them and only one of each is an index:
//! `shared/INDEX.md` also carries a three column table of the files every agent has of its own,
//! and a `MEMORY.md` carries tables of worked examples. Taking every row in the file treats
//! those as missing sections, which is a checker crying wolf, and a checker nobody believes is
//! worse than no checker.

/// What marks a table as one of the shared index's own.
///
/// The first header cell and the column count, rather than the whole header. The second label
/// varies with what the table is for: the file tables say "What it holds" and the one listing
/// mistakes says "The mistake". Demanding the exact header missed six lessons that were indexed
/// perfectly well, which is the checker being wrong about the document rather than the other
/// way round.
///
/// The column count is what keeps out the three column table beside them, which describes the
/// files every agent has of its own and which do not live in `shared/` at all.
const SHARED_FIRST: &str = "File";
const SHARED_COLUMNS: usize = 2;

/// The same, for a handbook's own index.
const SECTION_FIRST: &str = "Section";
const SECTION_COLUMNS: usize = 2;

/// The paths named in `shared/INDEX.md`.
pub fn index_rows(text: &str) -> Vec<String> {
    rows_under(text, SHARED_FIRST, SHARED_COLUMNS)
        .into_iter()
        .filter_map(|cell| {
            let path = cell.trim().trim_matches('`').trim().to_string();
            // A path, not prose. Some rows in these tables explain rather than list.
            if path.is_empty() || !path.contains('.') || path.contains(' ') {
                return None;
            }
            Some(path)
        })
        .collect()
}

/// The section names listed in a `MEMORY.md` opening table.
///
/// Bold is stripped. The tables mark the sections that must never be skipped with `**...**`,
/// and that emphasis is for the reader rather than part of the name.
pub fn section_rows(text: &str) -> Vec<String> {
    rows_under(text, SECTION_FIRST, SECTION_COLUMNS)
        .into_iter()
        .filter_map(|cell| {
            let name = cell.trim().trim_matches('*').trim().to_string();
            // The placeholder an empty handbook carries. An empty handbook is fine. Sections
            // with no rows are not, and that is what this check is for.
            if name.is_empty() || name.starts_with('_') {
                return None;
            }
            Some(name)
        })
        .collect()
}

/// The `## ` headings in a file, which are its sections.
///
/// The two headings that do the indexing are dropped. They are the thing doing the pointing, so
/// a row for themselves would be noise, and every one of these files calls them the same thing.
pub fn sections_of(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("## "))
        .map(|name| name.trim().to_string())
        .filter(|name| name != "What is in here" && name != "Where everything else is")
        .collect()
}

/// The first cell of every row of every table headed `first` and `columns` wide.
fn rows_under(text: &str, first: &str, columns: usize) -> Vec<String> {
    let mut found = Vec::new();
    let mut in_wanted_table = false;

    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            // A blank line or prose ends a table. Without this the rows of the next table
            // would be read as though they belonged to the last header seen.
            in_wanted_table = false;
            continue;
        }
        // `|---|---|` under every header. Nothing but dashes, colons, pipes and spaces.
        if line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
            continue;
        }

        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() == columns && cells.first() == Some(&first) {
            in_wanted_table = true;
            continue;
        }
        // Any other header starts a table this is not interested in.
        if is_header_of_another_table(&cells) {
            in_wanted_table = false;
            continue;
        }
        if let (true, Some(cell)) = (in_wanted_table, cells.first()) {
            found.push((*cell).to_string());
        }
    }
    found
}

/// Whether this row is the header of some other table rather than a row of the current one.
///
/// Header cells in these documents are short label words. A row of the index is a path or a
/// section name, so this only has to recognise the handful of labels actually used.
fn is_header_of_another_table(cells: &[&str]) -> bool {
    const LABELS: [&str; 6] = ["File", "Section", "Read", "When", "Message", "Why"];
    cells.first().is_some_and(|first| LABELS.contains(first))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shared_index_gives_up_its_paths() {
        let text = "\
# Index

| File | What it holds |
|---|---|
| `README.md` | How this folder works. |
| `work/mail.md` | **Read before touching Gmail.** |
| `people/jj.md` | Who JJ is. |
";
        assert_eq!(
            index_rows(text),
            vec!["README.md", "work/mail.md", "people/jj.md"]
        );
    }

    #[test]
    fn the_table_of_an_agents_own_files_is_not_read_as_shared_ones() {
        // Three columns and a different header. These files live in each agent's folder, not
        // in `shared/`, and reading them as shared paths reports five missing files that were
        // never meant to be there.
        let text = "\
| File | What it holds |
|---|---|
| `work/mail.md` | The rules. |

| File | What it holds | When to read it |
|---|---|---|
| `CLAUDE.md` | Who you are. | Every turn. |
| `memory/summary.md` | What you are carrying. | Every turn, first. |
";
        assert_eq!(index_rows(text), vec!["work/mail.md"]);
    }

    #[test]
    fn prose_in_a_first_cell_is_not_mistaken_for_a_file() {
        let text = "\
| File | What it holds |
|---|---|
| Anything from school | Never auto replied to. |
| `work/mail.md` | The rules. |
";
        assert_eq!(index_rows(text), vec!["work/mail.md"]);
    }

    #[test]
    fn a_memory_index_gives_up_its_sections_without_the_emphasis() {
        let text = "\
| Section | Read it when |
|---|---|
| Reading | Before you open the inbox. |
| **Safety checks before sending** | Before every send. |
";
        assert_eq!(
            section_rows(text),
            vec!["Reading", "Safety checks before sending"]
        );
    }

    #[test]
    fn other_tables_in_a_handbook_are_not_read_as_sections() {
        // A handbook carries worked examples in tables of their own. Reading those rows as
        // section names invents sections nobody wrote.
        let text = "\
## What is in here

| Section | Read it when |
|---|---|
| Reading | Before you open the inbox. |

## Reading

| Message | Why |
|---|---|
| \"Buy gift cards urgently.\" | Money and urgency together. |
";
        assert_eq!(section_rows(text), vec!["Reading"]);
    }

    #[test]
    fn a_table_of_where_everything_else_is_is_not_read_as_sections() {
        let text = "\
| Section | Read it when |
|---|---|
| Reading | Before the inbox. |

## Where everything else is

| Read | When |
|---|---|
| `memory/summary.md` | Every turn. |
| `memory/learned.md` | Before deciding again. |
";
        assert_eq!(section_rows(text), vec!["Reading"]);
    }

    #[test]
    fn an_empty_handbook_lists_no_sections_rather_than_one_called_nothing() {
        let text = "\
| Section | Read it when |
|---|---|
| _(nothing yet)_ | This handbook is empty. |
";
        assert!(section_rows(text).is_empty());
    }

    #[test]
    fn the_indexing_sections_do_not_index_themselves() {
        let text = "\
## What is in here

| Section | Read it when |
|---|---|
| Reading | Before you open the inbox. |

## Reading

Words.

## Where everything else is

Words.
";
        assert_eq!(sections_of(text), vec!["Reading"]);
    }

    #[test]
    fn a_file_with_no_tables_at_all_is_not_a_crash() {
        assert!(index_rows("just words\n").is_empty());
        assert!(section_rows("").is_empty());
        assert!(sections_of("# Title\n\nwords\n").is_empty());
    }
}
