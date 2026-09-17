/// Reverse a string.
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
pub fn title_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for ch in s.chars() {
        if capitalize_next && ch.is_alphabetic() {
            result.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
            if ch == ' ' {
                capitalize_next = true;
            }
        }
    }
    result
}

/// Count the number of words in a string (split on whitespace).
pub fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}
