/// Reverse a string, handling Unicode characters properly
pub fn reverse(s: &str) -> String {
    s.chars().rev().collect()
}

/// Truncate a string to max_len, adding "..." if truncated
pub fn truncate(s: &str, max_len: usize) -> String {
    // If string fits within max_len characters (inclusive), return as-is
    if s.chars().count() <= max_len {
        return s.to_string();
    }
    
    // Take max_len characters and add ellipsis
    let ellipsis = "...";
    let result: String = s.chars().take(max_len).collect();
    format!("{}{}", result, ellipsis)
}

/// Convert string to title case (capitalize first letter of each word)
pub fn title_case(s: &str) -> String {
    let mut result = String::new();
    let mut new_word = true;
    
    for c in s.chars() {
        if c.is_whitespace() {
            result.push(c);
            new_word = true;
        } else if new_word {
            result.extend(c.to_uppercase());
            new_word = false;
        } else {
            result.push(c);
        }
    }
    
    result
}

/// Count words in a string, ignoring extra whitespace
pub fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}
