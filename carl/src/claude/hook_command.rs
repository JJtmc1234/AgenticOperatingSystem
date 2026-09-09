//! Claude accepts a command string, so quote every argument for its POSIX shell.
use crate::panel::permission::{Verdict, decision};
use std::path::Path;

pub(super) fn deny(reason: &str) -> String {
    let json = decision(Verdict::Deny, reason);
    format!(
        "printf '%s\\n' {}",
        shlex::try_quote(&json).expect("JSON has no NUL bytes")
    )
}

pub(super) fn build(binary: &Path, home: &Path, surface: &str) -> String {
    let binary = super::executable::clean(binary);
    let Some(path) = binary.to_str() else {
        return deny("carl binary path is not valid UTF-8");
    };
    let Some(home) = home.to_str() else {
        return deny("carl home path is not valid UTF-8");
    };
    let arguments = [path, "--home", home, "permit-hook", "--as", surface];
    let quoted: Result<Vec<_>, _> = arguments.iter().map(|arg| shlex::try_quote(arg)).collect();
    let Ok(quoted) = quoted else {
        return deny("carl hook arguments contain a NUL byte");
    };
    let invocation = quoted
        .iter()
        .map(|q| q.as_ref())
        .collect::<Vec<_>>()
        .join(" ");
    let path = &quoted[0];
    let missing = deny(&format!("carl binary not found at {}", binary.display()));
    let not_executable = deny(&format!(
        "carl binary not executable at {}",
        binary.display()
    ));
    let failed = deny(&format!("carl hook failed at {}", binary.display()));
    // Capture stdout so a failed executable cannot leave a partial decision behind.
    // Returning deny with exit zero makes this a decision, not an ignored hook error.
    format!(
        "if [ ! -f {path} ]; then {missing}; \
        elif [ ! -x {path} ]; then {not_executable}; \
        elif carl_reply=$({invocation}); then \
        if [ -n \"$carl_reply\" ]; then printf '%s\\n' \"$carl_reply\"; else {failed}; fi; \
        else {failed}; fi"
    )
}
