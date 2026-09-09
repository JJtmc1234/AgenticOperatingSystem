//! Argument policies for inspection tools that accept file paths.

pub(super) fn accepts(program: &str, arguments: &[String]) -> Option<bool> {
    let (short, long, paths): (&str, &[&str], bool) = match program {
        "ls" => (
            "laAhdF1rStnispLH",
            &[
                "--all",
                "--almost-all",
                "--human-readable",
                "--directory",
                "--color=never",
                "--classify",
                "--numeric-uid-gid",
            ],
            true,
        ),
        "stat" => ("Lft", &["--dereference", "--file-system", "--terse"], true),
        "df" => (
            "hHiTPkm",
            &[
                "--human-readable",
                "--inodes",
                "--local",
                "--print-type",
                "--total",
            ],
            true,
        ),
        "du" => (
            "hskmcx",
            &[
                "--human-readable",
                "--summarize",
                "--total",
                "--one-file-system",
            ],
            true,
        ),
        "free" => (
            "hbkmgtw",
            &[
                "--human", "--bytes", "--kibi", "--mebi", "--gibi", "--total", "--wide",
            ],
            false,
        ),
        "id" => (
            "ugGnr",
            &["--user", "--group", "--groups", "--name", "--real"],
            false,
        ),
        _ => return None,
    };
    let mut literal_paths = false;
    for argument in arguments {
        if literal_paths {
            continue;
        }
        if argument == "--" && paths {
            literal_paths = true;
        } else if long.contains(&argument.as_str()) {
            continue;
        } else if program == "du" && argument.starts_with("--max-depth=") {
            if !argument[12..].parse::<u8>().is_ok_and(|depth| depth <= 10) {
                return Some(false);
            }
        } else if let Some(flags) = argument.strip_prefix('-') {
            if flags.is_empty() || !flags.chars().all(|flag| short.contains(flag)) {
                return Some(false);
            }
        } else if !paths {
            return Some(false);
        }
    }
    Some(true)
}
