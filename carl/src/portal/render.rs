//! Turning what the room holds into something a person or an agent reads.
//!
//! Kept apart from the client because it is the half worth testing hardest and the half that
//! needs no network at all. Every rule here is about a room that has both people and agents in
//! it, where the same transcript is read by a person on a screen and by an agent inside a
//! prompt.

use super::Said;

/// One message as a line.
///
/// The name comes first and is never omitted, even when the same person says three things in a
/// row. Chat clients collapse repeated names to look tidy. A transcript that does that and is
/// then read by an agent inside a prompt loses who said what, and an agent that misattributes
/// a line in a room with JJ's mentor in it is worse than a transcript that looks repetitive.
pub fn line_of(said: &Said) -> String {
    format!("{}: {}", said.who, one_line(&said.text))
}

/// Newlines flattened, because a line is a line.
///
/// A message with a newline in it would otherwise produce a second line with no name on it,
/// which reads as somebody else speaking. The text is not otherwise touched: it is not
/// truncated, not escaped and not tidied, because a record people trust shows what was said.
fn one_line(text: &str) -> String {
    text.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The whole room as text, oldest first.
///
/// Oldest first because it is read as a conversation rather than as a feed. Newest first is
/// right for something you scan for what changed, and wrong for something you have to follow.
pub fn transcript(said: &[Said]) -> String {
    said.iter().map(line_of).collect::<Vec<_>>().join("\n")
}

/// What to show after reading from a watermark, including when nothing is new.
///
/// Saying so plainly matters more here than it looks. `carl portal read` is run by an agent on
/// a schedule as well as by a person at a prompt, and an agent handed an empty string has to
/// guess whether the room was quiet or the call failed. Those need different answers.
pub fn since(said: &[Said]) -> String {
    if said.is_empty() {
        return "Nothing new in the room.".to_string();
    }
    transcript(said)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(id: i64, who: &str, text: &str) -> Said {
        Said {
            id,
            who: who.into(),
            text: text.into(),
            at: 1_788_000_000 + id,
        }
    }

    #[test]
    fn a_line_always_carries_its_name() {
        let one = said(1, "Carl", "the build is green");
        assert_eq!(line_of(&one), "Carl: the build is green");

        // Three from the same speaker keep three names. A reader inside a prompt has no
        // indentation to fall back on.
        let run = [
            said(1, "JJ", "one"),
            said(2, "JJ", "two"),
            said(3, "JJ", "three"),
        ];
        assert_eq!(transcript(&run), "JJ: one\nJJ: two\nJJ: three");
    }

    #[test]
    fn a_message_with_newlines_stays_one_line() {
        let one = said(1, "Hunter", "first\nsecond\n\nthird");
        assert_eq!(line_of(&one), "Hunter: first second third");
        // Otherwise the second line reads as somebody else speaking.
        assert_eq!(line_of(&one).lines().count(), 1);
    }

    #[test]
    fn the_text_itself_is_not_tidied() {
        // Long, punctuated, and holding something that looks like markup. A record people
        // trust shows what was said.
        let awkward = "he said \"no\" <b>twice</b> & meant it";
        let one = said(1, "JJ", awkward);
        assert!(line_of(&one).ends_with(awkward));
    }

    #[test]
    fn the_room_reads_oldest_first() {
        let room = [said(1, "JJ", "morning"), said(2, "Carl", "morning")];
        let text = transcript(&room);
        assert!(text.find("JJ: morning").unwrap() < text.find("Carl: morning").unwrap());
    }

    #[test]
    fn a_quiet_room_says_so_rather_than_nothing() {
        // An agent handed an empty string cannot tell a quiet room from a failed call.
        assert_eq!(since(&[]), "Nothing new in the room.");
        assert_eq!(since(&[said(1, "Carl", "hello")]), "Carl: hello");
    }

    #[test]
    fn an_empty_room_renders_as_nothing_rather_than_a_blank_line() {
        assert_eq!(transcript(&[]), "");
    }
}
