//! JSON Lines helpers.

/// Iterate over lines of a JSON Lines blob, skipping empty lines.
pub fn jsonl_lines(raw: &[u8]) -> impl Iterator<Item = &[u8]> {
    raw.split(|&b| b == b'\n')
        .filter(|l| !l.is_empty() && !l.iter().all(|&c| c == b' ' || c == b'\r' || c == b'\t'))
}

/// Truncate a string to at most `max` chars (UTF-8 safe).
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out = String::with_capacity(max + 16);
    let mut count = 0usize;
    for c in s.chars() {
        if count >= max {
            out.push_str("...[truncated]");
            break;
        }
        out.push(c);
        count += 1;
    }
    out
}
