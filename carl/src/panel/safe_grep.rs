//! Literal grep searches with explicit options and no shell composition.
pub(super) fn accepts(command: &str) -> bool {
    if command.len() > 4096 || command.contains(['\n', '\r', '\0']) {
        return false;
    }
    let mut quote = None;
    for c in command.chars() {
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                }
            }
            Some('"') => {
                if c == '"' {
                    quote = None;
                } else if matches!(c, '$' | '`' | '\\') {
                    return false;
                }
            }
            _ => {
                if matches!(c, '\'' | '"') {
                    quote = Some(c);
                } else if !c.is_ascii_alphanumeric() && !" \t_./-=:".contains(c) {
                    return false;
                }
            }
        }
    }
    if quote.is_some() {
        return false;
    }
    let Some(words) = shlex::split(command) else {
        return false;
    };
    if words.len() > 128
        || !words
            .first()
            .is_some_and(|p| matches!(p.as_str(), "grep" | "/bin/grep" | "/usr/bin/grep"))
    {
        return false;
    }
    let mut literal = false;
    let mut value = None;
    for arg in &words[1..] {
        if let Some(numeric) = value.take() {
            if numeric && !number(arg) {
                return false;
            }
            continue;
        }
        if literal {
            continue;
        }
        if arg == "--" {
            literal = true;
            continue;
        }
        if ["--regexp", "--include", "--exclude", "--exclude-dir"].contains(&arg.as_str()) {
            value = Some(false);
            continue;
        }
        if [
            "--after-context",
            "--before-context",
            "--context",
            "--max-count",
        ]
        .contains(&arg.as_str())
        {
            value = Some(true);
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            if let Some((key, val)) = long.split_once('=') {
                if ["regexp", "include", "exclude", "exclude-dir"].contains(&key) {
                    continue;
                }
                if ["after-context", "before-context", "context", "max-count"].contains(&key)
                    && number(val)
                {
                    continue;
                }
                return false;
            }
            if ![
                "recursive",
                "dereference-recursive",
                "line-number",
                "ignore-case",
                "invert-match",
                "word-regexp",
                "line-regexp",
                "extended-regexp",
                "fixed-strings",
                "count",
                "files-with-matches",
                "files-without-match",
                "only-matching",
                "no-messages",
                "with-filename",
                "no-filename",
                "text",
                "quiet",
            ]
            .contains(&long)
            {
                return false;
            }
        } else if let Some(short) = arg.strip_prefix('-') {
            if short.is_empty() {
                return false;
            }
            for (i, flag) in short.char_indices() {
                if "ABCme".contains(flag) {
                    let rest = &short[i + 1..];
                    if rest.is_empty() {
                        value = Some(flag != 'e');
                    } else if flag != 'e' && !number(rest) {
                        return false;
                    }
                    break;
                }
                if !"rRnivwxEFclLoqsHhIa".contains(flag) {
                    return false;
                }
            }
        }
    }
    value.is_none()
}

fn number(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.parse::<u32>().is_ok_and(|n| n <= 10000)
}
