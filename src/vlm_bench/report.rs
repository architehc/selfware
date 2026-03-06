//! JSON + Markdown report output for VLM benchmark results.

use serde::{Deserialize, Serialize};

use super::scoring::{LevelScore, Rating};
use super::Difficulty;

/// Report for a single benchmark level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelReport {
    /// Level name.
    pub name: String,
    /// Difficulty tier.
    pub difficulty: Difficulty,
    /// Level description.
    pub description: String,
    /// Number of scenarios evaluated.
    pub scenario_count: usize,
    /// Average accuracy across scenarios (0.0–1.0).
    pub score: f64,
    /// Garden-aesthetic rating.
    pub rating: Rating,
    /// Total tokens consumed.
    pub total_tokens: u64,
    /// Average latency per scenario in milliseconds.
    pub avg_latency_ms: f64,
    /// Individual scenario scores.
    pub scores: Vec<LevelScore>,
}

/// Complete benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchReport {
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Model identifier.
    pub model: String,
    /// Endpoint used.
    pub endpoint: String,
    /// Per-level reports.
    pub levels: Vec<LevelReport>,
    /// Overall score (average of level scores).
    pub overall_score: f64,
    /// Overall rating.
    pub overall_rating: Rating,
    /// Total tokens consumed across all levels.
    pub total_tokens: u64,
    /// Total wall-clock duration in seconds.
    pub total_duration_secs: f64,
}

impl BenchReport {
    /// Serialize to JSON string.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Render as a Markdown summary table.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str(&format!(
            "# VLM Benchmark Report — {} \n",
            self.model
        ));
        md.push_str(&format!(
            "**Date**: {} | **Endpoint**: {}\n\n",
            self.timestamp, self.endpoint
        ));

        md.push_str(
            "| Level | Difficulty | Score | Rating | Tokens | Avg Latency |\n",
        );
        md.push_str(
            "|-------|-----------|-------|--------|--------|-------------|\n",
        );

        for level in &self.levels {
            md.push_str(&format!(
                "| {} | {} | {:.0}% | {} | {} | {:.1}s |\n",
                level.name,
                level.difficulty,
                level.score * 100.0,
                rating_with_emoji(level.rating),
                format_tokens(level.total_tokens),
                level.avg_latency_ms / 1000.0,
            ));
        }

        md.push_str(&format!(
            "\n**Overall**: {:.0}% — {}\n",
            self.overall_score * 100.0,
            rating_with_emoji(self.overall_rating),
        ));
        md.push_str(&format!(
            "**Total tokens**: {} | **Duration**: {:.1}s\n",
            format_tokens(self.total_tokens),
            self.total_duration_secs,
        ));

        md
    }

    /// Write report files to the output directory.
    pub fn write_to_dir(&self, dir: &std::path::Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;

        let json_path = dir.join("vlm_benchmark_report.json");
        let md_path = dir.join("vlm_benchmark_report.md");

        std::fs::write(&json_path, self.to_json()?)?;
        std::fs::write(&md_path, self.to_markdown())?;

        Ok(())
    }
}

/// Format a token count with thousand separators.
fn format_tokens(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

fn rating_with_emoji(rating: Rating) -> String {
    match rating {
        Rating::Bloom => "BLOOM \u{1F338}".into(),
        Rating::Grow => "GROW \u{1F33F}".into(),
        Rating::Wilt => "WILT \u{1F940}".into(),
        Rating::Frost => "FROST \u{2744}\u{FE0F}".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> BenchReport {
        BenchReport {
            timestamp: "2026-03-06T12:00:00Z".into(),
            model: "qwen/qwen3.5-9b".into(),
            endpoint: "http://192.168.1.99:1234/v1".into(),
            levels: vec![
                LevelReport {
                    name: "L1 TUI State".into(),
                    difficulty: Difficulty::Easy,
                    description: "Terminal state recognition".into(),
                    scenario_count: 4,
                    score: 0.92,
                    rating: Rating::Bloom,
                    total_tokens: 12340,
                    avg_latency_ms: 3200.0,
                    scores: vec![],
                },
                LevelReport {
                    name: "L2 Diagnostics".into(),
                    difficulty: Difficulty::Medium,
                    description: "Compiler diagnostics".into(),
                    scenario_count: 3,
                    score: 0.78,
                    rating: Rating::Bloom,
                    total_tokens: 18200,
                    avg_latency_ms: 5100.0,
                    scores: vec![],
                },
            ],
            overall_score: 0.85,
            overall_rating: Rating::Bloom,
            total_tokens: 30540,
            total_duration_secs: 42.3,
        }
    }

    #[test]
    fn test_report_to_json() {
        let report = sample_report();
        let json = report.to_json().unwrap();
        assert!(json.contains("qwen/qwen3.5-9b"));
        assert!(json.contains("L1 TUI State"));
        assert!(json.contains("L2 Diagnostics"));
    }

    #[test]
    fn test_report_to_markdown() {
        let report = sample_report();
        let md = report.to_markdown();
        assert!(md.contains("# VLM Benchmark Report"));
        assert!(md.contains("L1 TUI State"));
        assert!(md.contains("L2 Diagnostics"));
        assert!(md.contains("BLOOM"));
        assert!(md.contains("92%"));
        assert!(md.contains("78%"));
        assert!(md.contains("Overall"));
    }

    #[test]
    fn test_report_serde_roundtrip() {
        let report = sample_report();
        let json = serde_json::to_string(&report).unwrap();
        let parsed: BenchReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, report.model);
        assert_eq!(parsed.levels.len(), report.levels.len());
        assert!((parsed.overall_score - report.overall_score).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rating_with_emoji() {
        assert!(rating_with_emoji(Rating::Bloom).contains("BLOOM"));
        assert!(rating_with_emoji(Rating::Grow).contains("GROW"));
        assert!(rating_with_emoji(Rating::Wilt).contains("WILT"));
        assert!(rating_with_emoji(Rating::Frost).contains("FROST"));
    }
}
