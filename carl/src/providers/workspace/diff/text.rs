/// A readable difference between two pieces of text.
///
/// Trims the identical start and end, then shows what is left as removed and added blocks.
pub fn simple_diff(before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }

    let old: Vec<&str> = before.split_inclusive('\n').collect();
    let new: Vec<&str> = after.split_inclusive('\n').collect();

    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();

    // The tail is measured on what is left after the head, so a short file cannot have the
    // same line counted at both ends.
    let most = old.len().min(new.len()) - head;
    let tail = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(most);

    let mut out = String::new();
    out.push_str(&format!("@@ line {} @@\n", head + 1));
    for line in &old[head..old.len() - tail] {
        append_line(&mut out, '-', line);
    }
    for line in &new[head..new.len() - tail] {
        append_line(&mut out, '+', line);
    }
    out
}

fn append_line(out: &mut String, prefix: char, raw: &str) {
    let (text, ending) = if let Some(text) = raw.strip_suffix("\r\n") {
        (text, Some("\\ Line ending: CRLF\n"))
    } else if let Some(text) = raw.strip_suffix('\n') {
        (text, None)
    } else {
        (raw, Some("\\ No newline at end of file\n"))
    };
    out.push(prefix);
    out.push_str(text);
    out.push('\n');
    if let Some(ending) = ending {
        out.push_str(ending);
    }
}
