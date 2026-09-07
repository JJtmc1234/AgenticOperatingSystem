use super::*;

/// Both are appended after the memory notes, so neither may depend on being first.
#[test]
fn neither_brief_refers_to_its_own_position() {
    for b in [IDENTITY, SPOKEN] {
        assert!(!b.contains("above"), "{b}");
        assert!(!b.contains("below"), "{b}");
    }
}

/// The reason this file exists. Naming the failure is what made it stop happening, so a
/// future edit that softens it into a general "be helpful" should fail here.
#[test]
fn the_identity_says_plainly_that_coding_is_not_the_limit() {
    let lower = IDENTITY.to_lowercase();
    assert!(lower.contains("not a coding assistant"));
    assert!(lower.contains("general purpose"));
}

/// A brief about brevity that is itself enormous is both funny and a real cost, since it
/// rides along on every single turn.
#[test]
fn they_are_short_enough_to_send_every_turn() {
    assert!(spoken().len() < 5000, "{} chars", spoken().len());
}

/// Carl is called a helper that remembers, and until this was added his memory directory
/// stayed empty in real use. The instruction is the feature.
#[test]
fn he_is_told_how_to_remember_something() {
    assert!(IDENTITY.contains(crate::remember::MARKER));
    assert!(
        IDENTITY.contains(crate::remember::FORGET),
        "and how to drop it again, since a wrong note is worse than no note"
    );
    assert!(
        IDENTITY.contains(crate::remember::SEEN),
        "and where a game state goes, which is not permanent memory"
    );
    let lower = IDENTITY.to_lowercase();
    assert!(lower.contains("strict"), "and told to be sparing about it");
}

/// Arithmetic is what a model is most confidently wrong about, and the fix is telling it
/// to compute rather than telling it to try harder.
#[test]
fn it_is_told_to_compute_rather_than_estimate() {
    assert!(IDENTITY.to_lowercase().contains("python"));
}

/// JJ is graded on this and it is the one rule that shows up in everything Carl writes.
#[test]
fn the_house_style_is_passed_on() {
    assert!(IDENTITY.contains("Do not use dashes or semicolons"));
    for b in [IDENTITY, SPOKEN] {
        assert!(!b.contains('\u{2014}'), "the brief must obey its own rule");
        assert!(!b.contains(';'), "the brief must obey its own rule");
    }
}

#[test]
fn the_chief_is_told_to_delegate_work_through_his_leads() {
    for name in ["Adrian", "Mason", "Olivia", "Serena", "Rowan"] {
        assert!(IDENTITY.contains(name), "missing lead {name}");
    }
    assert!(IDENTITY.contains("carl handoff --from carl --to <lead>"));
    assert!(IDENTITY.contains("Miles reports to Olivia"));
    assert!(IDENTITY.contains("Work goes down the chain. Questions do not."));
    assert!(IDENTITY.contains("say so and stop rather than picking the work up"));
}

#[test]
fn iris_work_stays_in_carl_chat_and_goes_through_adrian() {
    assert!(CAPABILITIES.contains("carl handoff --from carl --to adrian"));
    assert!(CAPABILITIES.contains("Adrian delegates to Iris"));
    assert!(CAPABILITIES.contains("Keep the work in this Carl conversation"));
    assert!(CAPABILITIES.contains("A draft is not a published issue"));
}
