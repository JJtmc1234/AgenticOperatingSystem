//! Cleaning and word level tokenization.
//!
//! The tokenizer keeps the original case. Punctuation is split off into its own
//! token so the model can learn where a sentence ends and generation can put
//! the period back in the right place.

/// Collapse whitespace and fold a few unicode look alikes to plain ascii.
/// The result is one line with single spaces and no leading or trailing space.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if ch.is_control() {
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        match ch {
            '\u{2018}' | '\u{2019}' | '\u{02bc}' => out.push('\''),
            '\u{201c}' | '\u{201d}' => out.push('"'),
            '\u{2013}' | '\u{2014}' | '\u{2212}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            _ => out.push(ch),
        }
    }
    out
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Split cleaned text into word and punctuation tokens.
/// An apostrophe or a hyphen stays inside a word when letters sit on both
/// sides, so "don't" and "well-read" survive as single tokens.
pub fn words(text: &str) -> Vec<String> {
    let chars: Vec<char> = normalize(text).chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut buf = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            flush(&mut buf, &mut out);
        } else if is_word_char(c) {
            buf.push(c);
        } else if (c == '\'' || c == '-')
            && !buf.is_empty()
            && chars.get(i + 1).copied().is_some_and(is_word_char)
        {
            buf.push(c);
        } else {
            flush(&mut buf, &mut out);
            out.push(c.to_string());
        }
    }
    flush(&mut buf, &mut out);
    out
}

fn flush(buf: &mut String, out: &mut Vec<String>) {
    if !buf.is_empty() {
        out.push(std::mem::take(buf));
    }
}

/// Punctuation that hugs the word before it.
const CLOSERS: &str = ".,!?:;)]}%";
/// Punctuation that hugs the word after it.
const OPENERS: &str = "([{$";

/// Put tokens back together as readable text. This is the inverse of words for
/// everything except spacing that was never meaningful.
pub fn join(tokens: &[String]) -> String {
    let mut out = String::new();
    let mut quote_open = false;
    let mut glue_next = false;
    for tok in tokens {
        let one = tok.chars().count() == 1;
        let c = tok.chars().next().unwrap_or(' ');
        let mut glue_left = one && CLOSERS.contains(c);
        let mut opens = one && OPENERS.contains(c);
        if one && c == '"' {
            // A double quote closes if one is already open, otherwise it opens.
            glue_left = quote_open;
            opens = !quote_open;
            quote_open = !quote_open;
        }
        if !out.is_empty() && !glue_left && !glue_next {
            out.push(' ');
        }
        out.push_str(tok);
        glue_next = opens;
    }
    out
}
