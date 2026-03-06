//! Criterion benchmark entry point for the VLM benchmark suite.
//!
//! Run with: `cargo bench --features vlm-bench`
//!
//! Note: This benchmarks the scoring and evaluation pipeline, not the VLM calls.
//! For live VLM benchmarks, use the integration test: `cargo test --features vlm-bench -- --ignored vlm_integration`

use criterion::{criterion_group, criterion_main, Criterion};

use selfware::vlm_bench::scoring::{
    json_field_accuracy, keyword_accuracy, keyword_overlap_score, pearson_correlation, Rating,
};
use selfware::vlm_bench::{BenchScenario, Difficulty, ExpectedAnswer};

fn bench_keyword_accuracy(c: &mut Criterion) {
    let response = "The dashboard panel shows a loading spinner with error status \
                    and the help menu is visible in the sidebar with multiple widgets \
                    including a chart, table, and status bar";
    let keywords: Vec<String> = vec![
        "dashboard",
        "panel",
        "spinner",
        "error",
        "help",
        "sidebar",
        "chart",
        "table",
        "status",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    c.bench_function("keyword_accuracy_9_keywords", |b| {
        b.iter(|| keyword_accuracy(response, &keywords))
    });
}

fn bench_json_field_accuracy(c: &mut Criterion) {
    let response = r#"Based on my analysis, here are the results:
    {"active_panel": "dashboard", "status": "ok", "widget_count": 5,
     "theme": "dark", "has_error": false, "error_type": null}
    That covers the main observations."#;
    let expected = serde_json::json!({
        "active_panel": "dashboard",
        "status": "ok",
        "widget_count": 5,
        "theme": "dark"
    });

    c.bench_function("json_field_accuracy_4_fields", |b| {
        b.iter(|| json_field_accuracy(response, &expected))
    });
}

fn bench_keyword_overlap(c: &mut Criterion) {
    let response = "The architecture shows a daemon process that orchestrates sandbox evaluation \
                    using fitness metrics. The AST tools handle mutation while the tournament \
                    module performs selection based on composite scores.";
    let reference = "daemon sandbox fitness mutation tournament selection evaluation";

    c.bench_function("keyword_overlap_score", |b| {
        b.iter(|| keyword_overlap_score(response, reference))
    });
}

fn bench_pearson_correlation(c: &mut Criterion) {
    let predicted: Vec<f64> = vec![75.0, 70.0, 80.0, 72.0, 68.0];
    let actual: Vec<f64> = vec![78.0, 65.0, 82.0, 70.0, 72.0];

    c.bench_function("pearson_correlation_5_dim", |b| {
        b.iter(|| pearson_correlation(&predicted, &actual))
    });
}

fn bench_rating_from_accuracy(c: &mut Criterion) {
    let thresholds: Vec<(f64, f64)> = vec![
        (0.95, 0.80),
        (0.70, 0.80),
        (0.45, 0.80),
        (0.20, 0.80),
        (0.60, 0.50),
    ];

    c.bench_function("rating_from_accuracy_5_cases", |b| {
        b.iter(|| {
            for &(acc, threshold) in &thresholds {
                let _ = Rating::from_accuracy(acc, threshold);
            }
        })
    });
}

fn bench_scenario_serde(c: &mut Criterion) {
    let scenario = BenchScenario {
        id: "l1_dashboard_normal".into(),
        description: "Identify the active panel and status".into(),
        image_path: std::path::PathBuf::from("vlm_fixtures/l1_tui_state/dashboard_normal.png"),
        prompt: "Analyze this terminal UI screenshot and respond with JSON".into(),
        expected: ExpectedAnswer::JsonFields(serde_json::json!({
            "active_panel": "dashboard",
            "theme": "dark",
            "widget_count": 5
        })),
    };

    c.bench_function("scenario_serialize", |b| {
        b.iter(|| serde_json::to_string(&scenario).unwrap())
    });

    let json = serde_json::to_string(&scenario).unwrap();
    c.bench_function("scenario_deserialize", |b| {
        b.iter(|| serde_json::from_str::<BenchScenario>(&json).unwrap())
    });
}

criterion_group!(
    vlm_benches,
    bench_keyword_accuracy,
    bench_json_field_accuracy,
    bench_keyword_overlap,
    bench_pearson_correlation,
    bench_rating_from_accuracy,
    bench_scenario_serde,
);

criterion_main!(vlm_benches);
