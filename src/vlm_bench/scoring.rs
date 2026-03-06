//! Scoring and evaluation for VLM benchmark responses.
//!
//! Supports keyword matching, BM25 text similarity, JSON field extraction,
//! and visual score correlation.

use serde::{Deserialize, Serialize};

/// Score for a single benchmark scenario within a level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelScore {
    /// Overall accuracy (0.0–1.0).
    pub accuracy: f64,
    /// Per-criterion breakdown: (criterion name, score).
    pub detail_scores: Vec<(String, f64)>,
    /// Number of response tokens consumed.
    pub response_tokens: u64,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// Garden-aesthetic rating derived from accuracy and pass threshold.
    pub rating: Rating,
}

/// Garden-aesthetic rating, mirroring `GenerationRating` from evolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rating {
    Bloom,
    Grow,
    Wilt,
    Frost,
}

impl std::fmt::Display for Rating {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bloom => write!(f, "BLOOM"),
            Self::Grow => write!(f, "GROW"),
            Self::Wilt => write!(f, "WILT"),
            Self::Frost => write!(f, "FROST"),
        }
    }
}

impl Rating {
    /// Derive a rating from accuracy and the level's pass threshold.
    pub fn from_accuracy(accuracy: f64, pass_threshold: f64) -> Self {
        if accuracy >= pass_threshold {
            Self::Bloom
        } else if accuracy >= pass_threshold * 0.75 {
            Self::Grow
        } else if accuracy >= pass_threshold * 0.5 {
            Self::Wilt
        } else {
            Self::Frost
        }
    }
}

/// Compute keyword match accuracy.
///
/// Returns the fraction of `expected` keywords found in `response` (case-insensitive).
pub fn keyword_accuracy(response: &str, expected: &[String]) -> f64 {
    if expected.is_empty() {
        return 1.0;
    }
    let response_lower = response.to_lowercase();
    let matched = expected
        .iter()
        .filter(|kw| response_lower.contains(&kw.to_lowercase()))
        .count();
    matched as f64 / expected.len() as f64
}

/// Score structured JSON field matches.
///
/// Checks if `response` contains a JSON object with fields matching `expected`.
/// Returns (accuracy, detail_scores) where each field is scored independently.
pub fn json_field_accuracy(
    response: &str,
    expected: &serde_json::Value,
) -> (f64, Vec<(String, f64)>) {
    let parsed = extract_json_from_response(response);
    let expected_obj = match expected.as_object() {
        Some(obj) => obj,
        None => return (0.0, vec![]),
    };

    let parsed_obj = match parsed.as_ref().and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => {
            let details: Vec<(String, f64)> =
                expected_obj.keys().map(|k| (k.clone(), 0.0)).collect();
            return (0.0, details);
        }
    };

    let mut details = Vec::new();
    let mut total = 0.0;

    for (key, expected_val) in expected_obj {
        let score = match parsed_obj.get(key) {
            Some(actual_val) => {
                if actual_val == expected_val {
                    1.0
                } else if let (Some(e), Some(a)) = (expected_val.as_str(), actual_val.as_str()) {
                    if e.to_lowercase() == a.to_lowercase() {
                        1.0
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
        details.push((key.clone(), score));
        total += score;
    }

    let accuracy = if expected_obj.is_empty() {
        1.0
    } else {
        total / expected_obj.len() as f64
    };
    (accuracy, details)
}

/// Compute simple keyword overlap score (lightweight BM25 alternative).
///
/// Tokenizes both texts, computes Jaccard-like overlap weighted by term frequency.
pub fn keyword_overlap_score(response: &str, reference: &str) -> f64 {
    let response_tokens = tokenize(response);
    let reference_tokens = tokenize(reference);

    if reference_tokens.is_empty() {
        return if response_tokens.is_empty() { 1.0 } else { 0.0 };
    }

    let matched = reference_tokens
        .iter()
        .filter(|t| response_tokens.contains(t))
        .count();

    matched as f64 / reference_tokens.len() as f64
}

/// Compute Pearson correlation between two score vectors.
///
/// Used for Mega level to compare VLM scores against ground-truth.
pub fn pearson_correlation(predicted: &[f64], actual: &[f64]) -> f64 {
    if predicted.len() != actual.len() || predicted.len() < 2 {
        return 0.0;
    }

    let n = predicted.len() as f64;
    let mean_p = predicted.iter().sum::<f64>() / n;
    let mean_a = actual.iter().sum::<f64>() / n;

    let mut cov = 0.0;
    let mut var_p = 0.0;
    let mut var_a = 0.0;

    for (p, a) in predicted.iter().zip(actual.iter()) {
        let dp = p - mean_p;
        let da = a - mean_a;
        cov += dp * da;
        var_p += dp * dp;
        var_a += da * da;
    }

    let denom = (var_p * var_a).sqrt();
    if denom < f64::EPSILON {
        return 0.0;
    }

    cov / denom
}

/// Extract a JSON object from a VLM response that may contain surrounding text.
fn extract_json_from_response(response: &str) -> Option<serde_json::Value> {
    let trimmed = response.trim();

    // Try direct parse first
    if let Ok(v) = serde_json::from_str(trimmed) {
        return Some(v);
    }

    // Find JSON object boundaries
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if start >= end {
        return None;
    }

    serde_json::from_str(&trimmed[start..=end]).ok()
}

/// Simple whitespace + punctuation tokenizer.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|s| !s.is_empty() && s.len() > 1)
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rating_from_accuracy() {
        // Pass threshold 0.8
        assert_eq!(Rating::from_accuracy(0.85, 0.8), Rating::Bloom);
        assert_eq!(Rating::from_accuracy(0.80, 0.8), Rating::Bloom);
        assert_eq!(Rating::from_accuracy(0.65, 0.8), Rating::Grow); // >= 0.6
        assert_eq!(Rating::from_accuracy(0.45, 0.8), Rating::Wilt); // >= 0.4
        assert_eq!(Rating::from_accuracy(0.30, 0.8), Rating::Frost); // < 0.4
    }

    #[test]
    fn test_rating_display() {
        assert_eq!(format!("{}", Rating::Bloom), "BLOOM");
        assert_eq!(format!("{}", Rating::Frost), "FROST");
    }

    #[test]
    fn test_keyword_accuracy_all_match() {
        let response = "The dashboard panel shows a loading spinner with error status";
        let expected = vec!["dashboard".into(), "panel".into(), "spinner".into()];
        let acc = keyword_accuracy(response, &expected);
        assert!((acc - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_keyword_accuracy_partial() {
        let response = "The dashboard shows data";
        let expected = vec!["dashboard".into(), "panel".into(), "spinner".into()];
        let acc = keyword_accuracy(response, &expected);
        assert!((acc - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_keyword_accuracy_case_insensitive() {
        let response = "DASHBOARD Panel SPINNER";
        let expected = vec!["dashboard".into(), "panel".into(), "spinner".into()];
        let acc = keyword_accuracy(response, &expected);
        assert!((acc - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_keyword_accuracy_empty_expected() {
        assert!((keyword_accuracy("anything", &[]) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_json_field_accuracy_full_match() {
        let response = r#"{"panel": "dashboard", "status": "ok"}"#;
        let expected = serde_json::json!({"panel": "dashboard", "status": "ok"});
        let (acc, details) = json_field_accuracy(response, &expected);
        assert!((acc - 1.0).abs() < f64::EPSILON);
        assert_eq!(details.len(), 2);
    }

    #[test]
    fn test_json_field_accuracy_partial() {
        let response = r#"{"panel": "dashboard", "status": "error"}"#;
        let expected = serde_json::json!({"panel": "dashboard", "status": "ok"});
        let (acc, _details) = json_field_accuracy(response, &expected);
        assert!((acc - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_json_field_accuracy_with_surrounding_text() {
        let response = r#"Based on my analysis: {"panel": "dashboard"} That's what I see."#;
        let expected = serde_json::json!({"panel": "dashboard"});
        let (acc, _) = json_field_accuracy(response, &expected);
        assert!((acc - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_json_field_accuracy_no_json() {
        let response = "This is just plain text with no JSON";
        let expected = serde_json::json!({"panel": "dashboard"});
        let (acc, _) = json_field_accuracy(response, &expected);
        assert!((acc - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_keyword_overlap() {
        let response = "the quick brown fox jumps over the lazy dog";
        let reference = "the quick brown fox";
        let score = keyword_overlap_score(response, reference);
        assert!((score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_keyword_overlap_no_match() {
        let response = "completely different words here";
        let reference = "the quick brown fox";
        let score = keyword_overlap_score(response, reference);
        assert!(score < 0.5);
    }

    #[test]
    fn test_pearson_correlation_perfect() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let r = pearson_correlation(&a, &b);
        assert!((r - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_pearson_correlation_inverse() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![5.0, 4.0, 3.0, 2.0, 1.0];
        let r = pearson_correlation(&a, &b);
        assert!((r - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_pearson_correlation_uncorrelated() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![3.0, 1.0, 3.0, 1.0, 3.0];
        let r = pearson_correlation(&a, &b);
        assert!(r.abs() < 0.5, "Expected low correlation, got {}", r);
    }

    #[test]
    fn test_pearson_correlation_short() {
        assert!((pearson_correlation(&[1.0], &[2.0])).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pearson_correlation_mismatched_len() {
        assert!((pearson_correlation(&[1.0, 2.0], &[3.0])).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_json_from_response() {
        let json = extract_json_from_response(r#"Sure! {"a": 1}"#);
        assert!(json.is_some());
        assert_eq!(json.unwrap()["a"], 1);
    }

    #[test]
    fn test_extract_json_no_json() {
        assert!(extract_json_from_response("no json here").is_none());
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("Hello, World! This is a test.");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"test".to_string()));
        // Single-char tokens filtered out
        assert!(!tokens.contains(&"a".to_string()));
    }

    #[test]
    fn test_level_score_serde() {
        let score = LevelScore {
            accuracy: 0.85,
            detail_scores: vec![("keywords".into(), 0.9), ("structure".into(), 0.8)],
            response_tokens: 1234,
            latency_ms: 3200,
            rating: Rating::Bloom,
        };
        let json = serde_json::to_string(&score).unwrap();
        let parsed: LevelScore = serde_json::from_str(&json).unwrap();
        assert!((parsed.accuracy - 0.85).abs() < f64::EPSILON);
        assert_eq!(parsed.rating, Rating::Bloom);
    }
}
