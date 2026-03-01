// Performance Optimization Challenge
//
// Each function below is correct but has deliberately bad algorithmic complexity.
// The goal is to replace each implementation with an optimal algorithm while
// preserving correctness. Performance tests enforce time budgets that only the
// optimized versions can meet.

/// O(n²) brute-force two-sum. Should be replaced with O(n) HashMap approach.
pub fn two_sum(nums: &[i32], target: i32) -> Option<(usize, usize)> {
    for i in 0..nums.len() {
        for j in (i + 1)..nums.len() {
            if nums[i] + nums[j] == target {
                return Some((i, j));
            }
        }
    }
    None
}

/// O(n²) unique word count using Vec::contains for dedup. Should use HashSet.
pub fn count_unique_words(text: &str) -> usize {
    let mut seen: Vec<&str> = Vec::new();
    for word in text.split_whitespace() {
        if !seen.contains(&word) {
            seen.push(word);
        }
    }
    seen.len()
}

/// Exponential recursive LCS without memoization. Should use DP table.
pub fn longest_common_subsequence(a: &str, b: &str) -> String {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    lcs_recursive(&a_chars, &b_chars, a_chars.len(), b_chars.len())
}

fn lcs_recursive(a: &[char], b: &[char], i: usize, j: usize) -> String {
    if i == 0 || j == 0 {
        return String::new();
    }
    if a[i - 1] == b[j - 1] {
        let mut result = lcs_recursive(a, b, i - 1, j - 1);
        result.push(a[i - 1]);
        result
    } else {
        let left = lcs_recursive(a, b, i - 1, j);
        let up = lcs_recursive(a, b, i, j - 1);
        if left.len() >= up.len() {
            left
        } else {
            up
        }
    }
}

/// O(n²) sort then nested-loop dedup. Should use sort + dedup() method.
pub fn sorted_unique(data: &mut Vec<i32>) -> Vec<i32> {
    // Bubble sort (O(n²))
    let n = data.len();
    for i in 0..n {
        for j in 0..n.saturating_sub(i + 1) {
            if data[j] > data[j + 1] {
                data.swap(j, j + 1);
            }
        }
    }

    // Nested-loop dedup (O(n²))
    let mut result: Vec<i32> = Vec::new();
    for &val in data.iter() {
        let mut found = false;
        for &existing in result.iter() {
            if existing == val {
                found = true;
                break;
            }
        }
        if !found {
            result.push(val);
        }
    }
    result
}

/// O(n*k) char frequency count — iterates text once per unique char. Should use HashMap.
pub fn char_frequencies(text: &str) -> Vec<(char, usize)> {
    let chars: Vec<char> = text.chars().collect();

    // Collect unique chars via linear scan (O(n*k))
    let mut unique: Vec<char> = Vec::new();
    for &c in &chars {
        if !unique.contains(&c) {
            unique.push(c);
        }
    }

    // Count each unique char by scanning the full text again (O(n*k))
    let mut result: Vec<(char, usize)> = Vec::new();
    for &u in &unique {
        let count = chars.iter().filter(|&&c| c == u).count();
        result.push((u, count));
    }

    result.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    // ---------------------------------------------------------------
    // Correctness tests (small inputs — pass even with slow impls)
    // ---------------------------------------------------------------

    #[test]
    fn test_two_sum_basic() {
        assert_eq!(two_sum(&[2, 7, 11, 15], 9), Some((0, 1)));
    }

    #[test]
    fn test_two_sum_no_match() {
        assert_eq!(two_sum(&[1, 2, 3], 100), None);
    }

    #[test]
    fn test_two_sum_negative() {
        // -3 + 5 = 2 is the only valid pair
        assert_eq!(two_sum(&[-3, 10, 5, 20], 2), Some((0, 2)));
    }

    #[test]
    fn test_unique_words_basic() {
        assert_eq!(count_unique_words("the cat sat on the mat"), 5);
    }

    #[test]
    fn test_unique_words_empty() {
        assert_eq!(count_unique_words(""), 0);
    }

    #[test]
    fn test_unique_words_all_same() {
        assert_eq!(count_unique_words("hello hello hello"), 1);
    }

    #[test]
    fn test_lcs_basic() {
        let result = longest_common_subsequence("abcde", "ace");
        assert_eq!(result, "ace");
    }

    #[test]
    fn test_lcs_no_common() {
        let result = longest_common_subsequence("abc", "xyz");
        assert_eq!(result, "");
    }

    #[test]
    fn test_lcs_identical() {
        let result = longest_common_subsequence("hello", "hello");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_sorted_unique_basic() {
        let mut data = vec![3, 1, 2, 1, 3];
        assert_eq!(sorted_unique(&mut data), vec![1, 2, 3]);
    }

    #[test]
    fn test_sorted_unique_empty() {
        let mut data: Vec<i32> = vec![];
        assert_eq!(sorted_unique(&mut data), vec![]);
    }

    #[test]
    fn test_sorted_unique_single() {
        let mut data = vec![42];
        assert_eq!(sorted_unique(&mut data), vec![42]);
    }

    #[test]
    fn test_char_freq_basic() {
        let freqs = char_frequencies("aabbc");
        // Sorted by descending count, then ascending char
        assert_eq!(freqs, vec![('a', 2), ('b', 2), ('c', 1)]);
    }

    #[test]
    fn test_char_freq_empty() {
        let freqs = char_frequencies("");
        assert!(freqs.is_empty());
    }

    #[test]
    fn test_char_freq_single() {
        let freqs = char_frequencies("z");
        assert_eq!(freqs, vec![('z', 1)]);
    }

    // ---------------------------------------------------------------
    // Performance tests (large inputs — only pass with optimal impls)
    // ---------------------------------------------------------------

    #[test]
    fn test_two_sum_large() {
        // 100_000 elements; target is sum of last two elements.
        let n = 100_000;
        let nums: Vec<i32> = (0..n).collect();
        let target = (n - 2) + (n - 1);

        let start = Instant::now();
        let result = two_sum(&nums, target);
        let elapsed = start.elapsed();

        assert_eq!(result, Some(((n - 2) as usize, (n - 1) as usize)));
        assert!(
            elapsed.as_millis() < 500,
            "two_sum took {}ms, budget is 500ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_unique_words_large() {
        // 50_000 words with ~5_000 unique.
        let vocab: Vec<String> = (0..5_000).map(|i| format!("word{}", i)).collect();
        let text: String = (0..50_000)
            .map(|i| vocab[i % vocab.len()].as_str())
            .collect::<Vec<&str>>()
            .join(" ");

        let start = Instant::now();
        let count = count_unique_words(&text);
        let elapsed = start.elapsed();

        assert_eq!(count, 5_000);
        assert!(
            elapsed.as_millis() < 500,
            "count_unique_words took {}ms, budget is 500ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_lcs_medium() {
        // Two 500-char strings. Exponential recursion would never finish.
        let a: String = (0..500).map(|i| (b'a' + (i % 26) as u8) as char).collect();
        let b: String = (0..500)
            .map(|i| (b'a' + ((i + 3) % 26) as u8) as char)
            .collect();

        let start = Instant::now();
        let result = longest_common_subsequence(&a, &b);
        let elapsed = start.elapsed();

        // The LCS should be non-trivial (exact length depends on pattern).
        assert!(!result.is_empty(), "LCS should not be empty for overlapping alphabets");
        assert!(
            elapsed.as_millis() < 2_000,
            "longest_common_subsequence took {}ms, budget is 2000ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_sorted_unique_large() {
        // 100_000 elements with duplicates.
        let mut data: Vec<i32> = (0..100_000).map(|i| i % 10_000).collect();

        let start = Instant::now();
        let result = sorted_unique(&mut data);
        let elapsed = start.elapsed();

        assert_eq!(result.len(), 10_000);
        assert_eq!(*result.first().unwrap(), 0);
        assert_eq!(*result.last().unwrap(), 9_999);
        // Verify sorted order
        for window in result.windows(2) {
            assert!(window[0] < window[1]);
        }
        assert!(
            elapsed.as_millis() < 500,
            "sorted_unique took {}ms, budget is 500ms",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_char_freq_large() {
        // 100_000 chars with moderate alphabet size.
        let text: String = (0..100_000)
            .map(|i| (b'a' + (i % 26) as u8) as char)
            .collect();

        let start = Instant::now();
        let freqs = char_frequencies(&text);
        let elapsed = start.elapsed();

        assert_eq!(freqs.len(), 26);
        let total: usize = freqs.iter().map(|(_, c)| c).sum();
        assert_eq!(total, 100_000);
        assert!(
            elapsed.as_millis() < 500,
            "char_frequencies took {}ms, budget is 500ms",
            elapsed.as_millis()
        );
    }
}
