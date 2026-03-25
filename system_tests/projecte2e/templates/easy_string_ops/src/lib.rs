/// Reverse a string.
///
/// Reverses characters properly, handling multi-byte UTF-8.
pub fn reverse(s: &str) -> String {
    s.chars().rev().collect()
}

/// Truncate a string to at most `max_len` characters, appending "..." if truncated.
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let prefix: String = s.chars().take(max_len).collect();
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
