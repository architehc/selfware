/// Reverse a string.
///
/// BUG: reverses bytes, not characters — breaks on multi-byte UTF-8.
pub fn reverse(s: &str) -> String {
    let bytes: Vec<u8> = s.bytes().rev().collect();
    String::from_utf8(bytes).unwrap_or_default()
}

/// Truncate a string to at most `max_len` characters, appending "..." if truncated.
///
/// BUG: off-by-one — truncates at max_len-1 instead of max_len.
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let prefix: String = s.chars().take(max_len.saturating_sub(1)).collect();
        format!("{}...", prefix)
    } else {
        s.to_string()
    }
}

/// Title-case: capitalize the first letter of each word.
///
/// BUG: only capitalizes the very first word, ignores the rest.
pub fn title_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut first = true;
    for ch in s.chars() {
        if first && ch.is_alphabetic() {
            result.extend(ch.to_uppercase());
            first = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Count the number of words in a string (split on whitespace).
///
/// BUG: counts empty splits when there are leading/trailing spaces.
pub fn word_count(s: &str) -> usize {
    s.split(' ').count()
}
