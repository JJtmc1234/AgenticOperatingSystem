use carl::heard::{Heard, interpret};
use carl::speech::Sentences;

#[test]
fn the_longer_wake_name_leaves_no_letter_in_the_question() {
    assert_eq!(
        interpret("Hey Carle, what do I do now?", false),
        Heard::Wake {
            question: Some("what do i do now".into())
        }
    );
    assert_eq!(
        interpret("Hey Carle", false),
        Heard::Wake { question: None }
    );
}

#[test]
fn a_wake_name_inside_an_unrelated_name_does_not_start_a_conversation() {
    assert_eq!(
        interpret("Hey Carlton, pass the ball", false),
        Heard::Nothing
    );
}

#[test]
fn a_stream_ending_at_or_inside_a_code_fence_does_not_repeat_or_read_code() {
    for text in [
        "Here:\n```rust\nlet x = 1;\n```",
        "Here:\n```rust\nlet secret = 1;",
    ] {
        let mut sentences = Sentences::new();
        let mut spoken = Vec::new();
        for ch in text.chars() {
            sentences.feed(&ch.to_string());
            while let Some(piece) = sentences.take() {
                spoken.push(piece);
            }
        }
        if let Some(tail) = sentences.rest() {
            spoken.push(tail);
        }
        assert_eq!(
            spoken.iter().filter(|s| s.contains("code block")).count(),
            1,
            "{spoken:?}"
        );
        assert!(!spoken.join(" ").contains("let "), "{spoken:?}");
    }
}
