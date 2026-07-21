use super::*;

#[test]
fn test_prompt_generation() {
    let snapshot = TelemetrySnapshot {
        hotspots: vec![
            CpuHotspot {
                function: "estimate_tokens".into(),
                file: "src/token_count.rs".into(),
                line: 47,
                cpu_percent: 42.3,
                call_count: 1_200_000,
                avg_duration_us: 0.8,
            },
            CpuHotspot {
                function: "parse_xml_tool_call".into(),
                file: "src/tool_parser.rs".into(),
                line: 182,
                cpu_percent: 18.7,
                call_count: 890_000,
                avg_duration_us: 2.1,
            },
        ],
        allocations: vec![AllocationHotspot {
            function: "parse_xml_tool_call".into(),
            allocs_per_call: 47,
            total_bytes: 2_400_000,
            peak_live_bytes: 512_000,
        }],
        benchmark_deltas: vec![
            BenchmarkDelta {
                name: "token_estimation".into(),
                baseline_ms: 0.23,
                current_ms: 0.25,
                delta_percent: 8.7,
            },
            BenchmarkDelta {
                name: "sab_easy_calc".into(),
                baseline_ms: 4.32,
                current_ms: 4.23,
                delta_percent: -2.1,
            },
        ],
        test_summary: TestSummary {
            total: 5200,
            passed: 5198,
            failed: 2,
            ignored: 0,
            duration: Duration::from_secs(240),
        },
    };

    let prompt = to_agent_prompt(&snapshot);

    // Verify the prompt contains actionable information
    assert!(prompt.contains("estimate_tokens"));
    assert!(prompt.contains("42.3%"));
    assert!(prompt.contains("token_estimation"));
    assert!(prompt.contains("SLOWER"));
    assert!(prompt.contains("FASTER") || prompt.contains("STABLE"));
    assert!(prompt.contains("5198/5200"));
}

#[test]
fn test_empty_snapshot() {
    let snapshot = TelemetrySnapshot {
        hotspots: vec![],
        allocations: vec![],
        benchmark_deltas: vec![],
        test_summary: TestSummary {
            total: 0,
            passed: 0,
            failed: 0,
            ignored: 0,
            duration: Duration::ZERO,
        },
    };
    let prompt = to_agent_prompt(&snapshot);
    assert!(prompt.contains("Performance Telemetry"));
    assert!(prompt.contains("0/0 passed"));
}

#[test]
fn test_parse_folded_stacks_basic() {
    let tmp = std::env::temp_dir().join("selfware-test-folded-basic.folded");
    std::fs::write(&tmp, "main;foo;bar 100\nmain;foo;baz 50\nmain;qux 25\n").unwrap();
    let hotspots = parse_folded_stacks(&tmp).unwrap();
    let _ = std::fs::remove_file(&tmp);

    assert_eq!(hotspots.len(), 3);
    // Sorted by CPU%, so "bar" (100/175 = 57.1%) should be first
    assert_eq!(hotspots[0].function, "bar");
    assert!((hotspots[0].cpu_percent - 100.0 / 175.0 * 100.0).abs() < 0.1);
    assert_eq!(hotspots[1].function, "baz");
    assert_eq!(hotspots[2].function, "qux");
}

#[test]
fn test_parse_folded_stacks_empty_file() {
    let tmp = std::env::temp_dir().join("selfware-test-folded-empty.folded");
    std::fs::write(&tmp, "").unwrap();
    let hotspots = parse_folded_stacks(&tmp).unwrap();
    let _ = std::fs::remove_file(&tmp);
    assert!(hotspots.is_empty());
}

#[test]
fn test_parse_folded_stacks_single_entry() {
    let tmp = std::env::temp_dir().join("selfware-test-folded-single.folded");
    std::fs::write(&tmp, "main;only_func 42\n").unwrap();
    let hotspots = parse_folded_stacks(&tmp).unwrap();
    let _ = std::fs::remove_file(&tmp);

    assert_eq!(hotspots.len(), 1);
    assert_eq!(hotspots[0].function, "only_func");
    assert!((hotspots[0].cpu_percent - 100.0).abs() < f64::EPSILON);
    assert_eq!(hotspots[0].call_count, 42);
}

#[test]
fn test_prompt_benchmark_thresholds() {
    let snapshot = TelemetrySnapshot {
        hotspots: vec![],
        allocations: vec![],
        benchmark_deltas: vec![
            BenchmarkDelta {
                name: "slow_bench".into(),
                baseline_ms: 1.0,
                current_ms: 1.05,
                delta_percent: 5.0, // > 2.0 → SLOWER (red)
            },
            BenchmarkDelta {
                name: "fast_bench".into(),
                baseline_ms: 1.0,
                current_ms: 0.90,
                delta_percent: -10.0, // < -2.0 → FASTER (green)
            },
            BenchmarkDelta {
                name: "stable_bench".into(),
                baseline_ms: 1.0,
                current_ms: 1.01,
                delta_percent: 1.0, // between -2 and 2 → STABLE
            },
        ],
        test_summary: TestSummary {
            total: 100,
            passed: 100,
            failed: 0,
            ignored: 0,
            duration: Duration::from_secs(10),
        },
    };
    let prompt = to_agent_prompt(&snapshot);
    assert!(prompt.contains("SLOWER"));
    assert!(prompt.contains("FASTER"));
    assert!(prompt.contains("STABLE"));
    assert!(prompt.contains("\u{1f534}")); // red circle
    assert!(prompt.contains("\u{1f7e2}")); // green circle
    assert!(prompt.contains("\u{26aa}")); // white circle
}

#[test]
fn test_prompt_many_hotspots_truncated() {
    let hotspots: Vec<CpuHotspot> = (0..15)
        .map(|i| CpuHotspot {
            function: format!("func_{}", i),
            file: format!("src/mod_{}.rs", i),
            line: i as u32,
            cpu_percent: 100.0 / 15.0,
            call_count: 1000,
            avg_duration_us: 1.0,
        })
        .collect();
    let snapshot = TelemetrySnapshot {
        hotspots,
        allocations: vec![],
        benchmark_deltas: vec![],
        test_summary: TestSummary {
            total: 0,
            passed: 0,
            failed: 0,
            ignored: 0,
            duration: Duration::ZERO,
        },
    };
    let prompt = to_agent_prompt(&snapshot);
    // Should contain func_0 through func_9 (10 items) but NOT func_10..14
    assert!(prompt.contains("func_0"));
    assert!(prompt.contains("func_9"));
    assert!(!prompt.contains("func_10"));
}

#[test]
fn test_prompt_many_allocations_truncated() {
    let allocations: Vec<AllocationHotspot> = (0..8)
        .map(|i| AllocationHotspot {
            function: format!("alloc_func_{}", i),
            allocs_per_call: 10,
            total_bytes: 1024,
            peak_live_bytes: 512,
        })
        .collect();
    let snapshot = TelemetrySnapshot {
        hotspots: vec![],
        allocations,
        benchmark_deltas: vec![],
        test_summary: TestSummary {
            total: 0,
            passed: 0,
            failed: 0,
            ignored: 0,
            duration: Duration::ZERO,
        },
    };
    let prompt = to_agent_prompt(&snapshot);
    // Should contain alloc_func_0 through alloc_func_4 (5 items) but NOT alloc_func_5+
    assert!(prompt.contains("alloc_func_0"));
    assert!(prompt.contains("alloc_func_4"));
    assert!(!prompt.contains("alloc_func_5"));
}

#[test]
fn test_parse_folded_stacks_malformed_lines() {
    let tmp = std::env::temp_dir().join("selfware-test-folded-malformed.folded");
    std::fs::write(
        &tmp,
        "main;valid_func 50\nno_count_here\n\nmain;another 30\n",
    )
    .unwrap();
    let hotspots = parse_folded_stacks(&tmp).unwrap();
    let _ = std::fs::remove_file(&tmp);

    // Should parse 2 valid entries, skip the malformed ones
    assert_eq!(hotspots.len(), 2);
}

#[test]
fn test_telemetry_error_display() {
    let e1 = TelemetryError::ToolFailed("flamegraph".into(), "not installed".into());
    assert!(format!("{}", e1).contains("flamegraph"));
    assert!(format!("{}", e1).contains("not installed"));

    let e2 = TelemetryError::ParseFailed("bad format".into());
    assert!(format!("{}", e2).contains("bad format"));
}
