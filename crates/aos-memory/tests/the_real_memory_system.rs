//! The checker, pointed at the memory system this repository actually ships.
//!
//! This is the test that makes `memory-system/README.md` true. It said the indexes were
//! "Checked, both directions" and nothing checked them, so a file could be added without its
//! row and no agent would ever find it, and nobody would know until an agent needed the rule
//! and could not.
//!
//! It runs against the real folder rather than a fixture on purpose. A fixture would prove the
//! parser works and would not stop the thing this exists to stop.

use std::path::PathBuf;

fn memory_system() -> PathBuf {
    // CARGO_MANIFEST_DIR is the crate. The memory system is at the workspace root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("memory-system")
}

#[test]
fn every_index_in_the_shipped_memory_system_is_true() {
    let faults = aos_memory::check(&memory_system()).expect("the memory system should be readable");
    assert!(
        faults.is_empty(),
        "the memory system's indexes are out of step with what is on disk:\n\n{}\n",
        faults
            .iter()
            .map(|f| format!("  {f}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn a_file_added_without_its_row_is_caught() {
    // The failure this exists for, proved by causing it. Copying the real folder rather than
    // writing a fixture keeps the proof about the real thing.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory-system");
    copy_into(&memory_system(), &root);
    assert!(
        aos_memory::check(&root).unwrap().is_empty(),
        "the copy should start clean"
    );

    std::fs::write(
        root.join("shared/work/a-new-rule.md"),
        "a rule nobody indexed",
    )
    .unwrap();

    let faults = aos_memory::check(&root).unwrap();
    assert_eq!(faults.len(), 1, "{faults:?}");
    let said = faults[0].to_string();
    assert!(said.contains("a-new-rule.md"), "{said}");
    assert!(said.contains("no agent will ever find it"), "{said}");
}

#[test]
fn a_row_left_behind_after_its_file_moves_is_caught() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory-system");
    copy_into(&memory_system(), &root);

    std::fs::remove_file(root.join("shared/work/committing.md")).unwrap();

    let faults = aos_memory::check(&root).unwrap();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert!(
        faults[0].to_string().contains("committing.md"),
        "{:?}",
        faults[0]
    );
}

#[test]
fn a_section_added_to_a_handbook_without_its_row_is_caught() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory-system");
    copy_into(&memory_system(), &root);

    let handbook = root.join("agents/olivia/miles/MEMORY.md");
    let mut text = std::fs::read_to_string(&handbook).unwrap();
    text.push_str("\n## Forwarding\n\nSomething nobody put in the index.\n");
    std::fs::write(&handbook, text).unwrap();

    let faults = aos_memory::check(&root).unwrap();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert!(
        faults[0].to_string().contains("Forwarding"),
        "{:?}",
        faults[0]
    );
}

#[test]
fn a_folder_that_is_not_a_memory_system_says_so() {
    let temp = tempfile::tempdir().unwrap();
    let err = aos_memory::check(temp.path()).unwrap_err().to_string();
    assert!(err.contains("shared/"), "{err}");
}

fn copy_into(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_into(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}
