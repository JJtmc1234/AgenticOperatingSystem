use sha2::{Digest, Sha256};

fn normalized(note: &str) -> String {
    note.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .to_lowercase()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub(crate) fn legacy_note_name(note: &str) -> String {
    normalized(note)
        .split('-')
        .take(6)
        .collect::<Vec<_>>()
        .join("-")
}

pub fn note_name(note: &str) -> String {
    let full = normalized(note);
    if full.split('-').count() <= 6 && full.len() <= 64 {
        return full;
    }
    // Keep familiar short names. Truncated names need a digest of the entire fact so
    // different endings cannot silently overwrite the same six word prefix.
    let prefix = legacy_note_name(note);
    let digest = format!("{:x}", Sha256::digest(full.as_bytes()));
    format!("{}-{}", &prefix[..prefix.len().min(31)], &digest[..32])
}
