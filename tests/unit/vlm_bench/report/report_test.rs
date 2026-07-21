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
