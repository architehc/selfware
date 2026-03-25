//! SWE-bench Patch Evaluator
//!
//! Evaluates generated patches against gold solutions and test results,
//! computing various quality metrics.

use anyhow::Result;
use std::collections::HashSet;

/// Evaluation metrics for a patch
#[derive(Debug, Clone)]
pub struct EvaluationMetrics {
    /// Overall quality score (0.0-1.0)
    pub overall_score: f64,
    /// Line-level precision
    pub line_precision: f64,
    /// Line-level recall
    pub line_recall: f64,
    /// File-level accuracy
    pub file_accuracy: f64,
    /// Levenshtein distance ratio
    pub edit_similarity: f64,
    /// Whether the right files were modified
    pub correct_files: bool,
    /// Whether tests pass
    pub tests_pass: bool,
}

/// Patch evaluator
pub struct PatchEvaluator {
    gold_patch: String,
    gold_files: Vec<String>,
}

impl PatchEvaluator {
    /// Create a new evaluator with gold patch
    pub fn new(gold_patch: String) -> Self {
        let gold_files = Self::extract_files_from_patch(&gold_patch);
        Self {
            gold_patch,
            gold_files,
        }
    }

    /// Evaluate a generated patch
    pub fn evaluate(&self, generated: &str) -> EvaluationMetrics {
        let gen_files = Self::extract_files_from_patch(generated);

        // Calculate file accuracy
        let correct_files = self.calculate_file_accuracy(&gen_files);
        let file_accuracy = if self.gold_files.is_empty() {
            if gen_files.is_empty() { 1.0 } else { 0.0 }
        } else {
            let intersection: HashSet<_> = self.gold_files.iter().collect();
            let generated: HashSet<_> = gen_files.iter().collect();
            let correct = intersection.intersection(&generated).count();
            correct as f64 / self.gold_files.len().max(1) as f64
        };

        // Calculate line metrics
        let (line_precision, line_recall) = self.calculate_line_metrics(generated);

        // Calculate edit similarity
        let edit_similarity = self.calculate_edit_similarity(generated);

        // Overall score (weighted average)
        let overall_score = 0.4 * file_accuracy + 0.3 * line_recall + 0.3 * edit_similarity;

        EvaluationMetrics {
            overall_score,
            line_precision,
            line_recall,
            file_accuracy,
            edit_similarity,
            correct_files,
            tests_pass: false, // Must be set externally
        }
    }

    /// Extract file paths from a unified diff
    fn extract_files_from_patch(patch: &str) -> Vec<String> {
        patch
            .lines()
            .filter(|l| l.starts_with("diff --git"))
            .filter_map(|l| {
                // "diff --git a/path/to/file b/path/to/file"
                let parts: Vec<&str> = l.split_whitespace().collect();
                parts.get(3).map(|p| p.trim_start_matches("b/").to_string())
            })
            .collect()
    }

    /// Calculate if files match gold
    fn calculate_file_accuracy(&self, generated: &[String]) -> bool {
        let gold_set: HashSet<_> = self.gold_files.iter().collect();
        let gen_set: HashSet<_> = generated.iter().collect();
        
        // At least one correct file and no wrong files
        let intersection: HashSet<_> = gold_set.intersection(&gen_set).collect();
        !intersection.is_empty() && gen_set.is_subset(&gold_set)
    }

    /// Calculate line-level precision and recall
    fn calculate_line_metrics(&self, generated: &str) -> (f64, f64) {
        // Extract added lines from gold patch (excluding context)
        let gold_added: HashSet<String> = self.gold_patch
            .lines()
            .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        // Extract added lines from generated patch
        let gen_added: HashSet<String> = generated
            .lines()
            .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        if gold_added.is_empty() && gen_added.is_empty() {
            return (1.0, 1.0);
        }

        let intersection: HashSet<_> = gold_added.intersection(&gen_added).collect();
        
        let precision = if gen_added.is_empty() {
            0.0
        } else {
            intersection.len() as f64 / gen_added.len() as f64
        };

        let recall = if gold_added.is_empty() {
            if gen_added.is_empty() { 1.0 } else { 0.0 }
        } else {
            intersection.len() as f64 / gold_added.len() as f64
        };

        (precision, recall)
    }

    /// Calculate edit similarity using Levenshtein distance
    fn calculate_edit_similarity(&self, generated: &str) -> f64 {
        let distance = levenshtein_distance(&self.gold_patch, generated);
        let max_len = self.gold_patch.len().max(generated.len());
        
        if max_len == 0 {
            return 1.0;
        }

        1.0 - (distance as f64 / max_len as f64)
    }

    /// Get the gold files
    pub fn gold_files(&self) -> &[String] {
        &self.gold_files
    }

    /// Check if patch touches the right files
    pub fn check_files(&self, generated: &str) -> bool {
        let gen_files = Self::extract_files_from_patch(generated);
        self.calculate_file_accuracy(&gen_files)
    }
}

/// Calculate Levenshtein distance between two strings
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.chars().count();
    let len2 = s2.chars().count();
    
    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    // Initialize first column and row
    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    for j in 0..=len2 {
        matrix[0][j] = j;
    }

    // Fill the matrix
    let chars1: Vec<char> = s1.chars().collect();
    let chars2: Vec<char> = s2.chars().collect();

    for i in 1..=len1 {
        for j in 1..=len2 {
            let cost = if chars1[i - 1] == chars2[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[len1][len2]
}

/// Compare two patches and generate a detailed diff report
pub fn generate_patch_comparison(gold: &str, generated: &str) -> String {
    let mut report = String::new();

    report.push_str("# Patch Comparison\n\n");

    // Extract files from both patches
    let gold_files = PatchEvaluator::extract_files_from_patch(gold);
    let gen_files = PatchEvaluator::extract_files_from_patch(generated);

    report.push_str("## Files Modified\n\n");
    report.push_str("| File | Gold | Generated |\n");
    report.push_str("|------|------|-----------|\n");

    let all_files: HashSet<_> = gold_files.iter()
        .chain(gen_files.iter())
        .collect();

    for file in all_files {
        let in_gold = gold_files.contains(file);
        let in_gen = gen_files.contains(file);
        let status = match (in_gold, in_gen) {
            (true, true) => "✅ Correct",
            (true, false) => "❌ Missing",
            (false, true) => "⚠️ Extra",
            (false, false) => "-",
        };
        report.push_str(&format!("| {} | {} | {} |\n", file, status, status));
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_evaluator_creation() {
        let gold = "diff --git a/file.py b/file.py\n--- a/file.py\n+++ b/file.py\n@@ -1 +1 @@\n-old\n+new";
        let evaluator = PatchEvaluator::new(gold.to_string());
        
        assert_eq!(evaluator.gold_files().len(), 1);
        assert_eq!(evaluator.gold_files()[0], "file.py");
    }

    #[test]
    fn test_evaluate_perfect_match() {
        let gold = "diff --git a/file.py b/file.py\n--- a/file.py\n+++ b/file.py\n@@ -1 +1 @@\n-old\n+new";
        let evaluator = PatchEvaluator::new(gold.to_string());
        let metrics = evaluator.evaluate(gold);

        assert!(metrics.overall_score > 0.9);
        assert!(metrics.file_accuracy > 0.9);
    }

    #[test]
    fn test_evaluate_wrong_file() {
        let gold = "diff --git a/file1.py b/file1.py\n--- a/file1.py\n+++ b/file1.py\n@@ -1 +1 @@\n-old\n+new";
        let generated = "diff --git a/file2.py b/file2.py\n--- a/file2.py\n+++ b/file2.py\n@@ -1 +1 @@\n-old\n+new";
        
        let evaluator = PatchEvaluator::new(gold.to_string());
        let metrics = evaluator.evaluate(generated);

        assert!(metrics.file_accuracy < 0.5);
        assert!(!metrics.correct_files);
    }

    #[test]
    fn test_line_metrics() {
        let gold = "diff --git a/file.py b/file.py\n--- a/file.py\n+++ b/file.py\n@@ -1,3 +1,3 @@\n line1\n-old_line\n+new_line\n line3";
        let generated = "diff --git a/file.py b/file.py\n--- a/file.py\n+++ b/file.py\n@@ -1,3 +1,3 @@\n line1\n-old_line\n+new_line\n line3";
        
        let evaluator = PatchEvaluator::new(gold.to_string());
        let (precision, recall) = evaluator.calculate_line_metrics(generated);

        assert!(precision > 0.9);
        assert!(recall > 0.9);
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("a", ""), 1);
        assert_eq!(levenshtein_distance("", "a"), 1);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("abc", "abd"), 1);
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn test_extract_files_from_patch() {
        let patch = "diff --git a/src/file1.py b/src/file1.py\ndiff --git a/src/file2.py b/src/file2.py";
        let files = PatchEvaluator::extract_files_from_patch(patch);
        
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "src/file1.py");
        assert_eq!(files[1], "src/file2.py");
    }
}
