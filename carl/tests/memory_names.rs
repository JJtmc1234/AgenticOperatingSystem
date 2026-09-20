use carl::{Memory, remember::note_name};

#[test]
fn different_facts_with_the_same_first_six_words_keep_separate_notes() {
    let home = tempfile::tempdir().unwrap();
    let memory = Memory::open(home.path()).unwrap();
    let first = "JJ wants the next dashboard to show agent work";
    let second = "JJ wants the next dashboard to hide unused metrics";
    let first_name = note_name(first);
    let second_name = note_name(second);
    assert_ne!(first_name, second_name);
    let first_path = memory.write_from(&first_name, first, "JJ").unwrap();
    let second_path = memory.write_from(&second_name, second, "JJ").unwrap();
    assert!(std::fs::read_to_string(first_path).unwrap().contains(first));
    assert!(
        std::fs::read_to_string(second_path)
            .unwrap()
            .contains(second)
    );
    assert_eq!(memory.notes().unwrap().len(), 2);
}

#[test]
fn long_fact_names_fit_the_memory_store_and_are_stable() {
    let home = tempfile::tempdir().unwrap();
    let memory = Memory::open(home.path()).unwrap();
    for fact in ["a".repeat(300), "configuration ".repeat(15)] {
        let name = note_name(&fact);
        assert!(name.len() <= 64, "{} bytes", name.len());
        assert_eq!(name, note_name(&format!("  {fact}!  ")));
        assert_ne!(name, note_name(&format!("{fact} different")));
        memory.write_from(&name, &fact, "JJ").unwrap();
    }
}
