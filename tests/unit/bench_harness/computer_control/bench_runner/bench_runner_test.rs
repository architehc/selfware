use super::*;

#[test]
fn test_browser_bench_config_default() {
    let cfg = BrowserBenchConfig::default();
    assert_eq!(cfg.harness.max_concurrent, 16);
    assert_eq!(cfg.max_browser_concurrent, 4);
    assert!(cfg.llm_analysis);
    assert!(cfg.store_traces);
}

#[test]
fn test_browser_bench_report_markdown() {
    let report = BrowserBenchReport {
        timestamp: "2026-03-25T00:00:00Z".into(),
        tasks_total: 2,
        tasks_passed: 1,
        traces: vec![],
        llm_report: None,
        total_duration_secs: 10.5,
    };
    let md = report.to_markdown();
    assert!(md.contains("Browser Benchmark Report"));
    assert!(md.contains("10.5s"));
}

#[test]
fn truncate_chars_is_utf8_safe() {
    // Each 'é' is 2 bytes; truncating at 3000 chars must not panic.
    let s = "é".repeat(5000);
    let t = truncate_chars(&s, 3000);
    assert_eq!(t.chars().count(), 3000);
    // shorter-than-max returns the whole string
    assert_eq!(truncate_chars("abc", 10), "abc");
    // mixed widths: a=1, é=2, 漢=3, 🎉=4 bytes — a byte slice here would panic
    let mixed = "aé漢🎉";
    assert_eq!(truncate_chars(mixed, 2), "aé");
    assert_eq!(truncate_chars(mixed, 3), "aé漢");
    assert_eq!(truncate_chars(mixed, 99), mixed);
}

#[test]
fn is_image_path_detects_screenshots() {
    use std::path::Path;
    assert!(is_image_path(Path::new("/x/shot.png")));
    assert!(is_image_path(Path::new("/x/shot.PNG")));
    assert!(is_image_path(Path::new("a.jpeg")));
    assert!(!is_image_path(Path::new("/x/page.html")));
    assert!(!is_image_path(Path::new("/x/data.json")));
    assert!(!is_image_path(Path::new("/x/noext")));
}
