//! Telemetry — Sensory Organs for the Evolution Daemon
//!
//! Exposes CPU profiling, memory allocation, and benchmark data in a format
//! the agent can consume as working memory context. This gives the LLM a
//! gradient signal to guide mutations toward actual bottlenecks rather than
//! blind search.

#![allow(dead_code, unused_imports, unused_variables)]

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct TelemetrySnapshot {
    pub hotspots: Vec<CpuHotspot>,
    pub allocations: Vec<AllocationHotspot>,
    pub benchmark_deltas: Vec<BenchmarkDelta>,
    pub test_summary: TestSummary,
}

#[derive(Debug, Clone)]
pub struct CpuHotspot {
    pub function: String,
    pub file: String,
    pub line: u32,
    pub cpu_percent: f64,
    pub call_count: u64,
    pub avg_duration_us: f64,
}

#[derive(Debug, Clone)]
pub struct AllocationHotspot {
    pub function: String,
    pub allocs_per_call: u64,
    pub total_bytes: u64,
    pub peak_live_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct BenchmarkDelta {
    pub name: String,
    pub baseline_ms: f64,
    pub current_ms: f64,
    pub delta_percent: f64,
}

#[derive(Debug, Clone)]
pub struct TestSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub ignored: usize,
    pub duration: Duration,
}

/// Capture a full telemetry snapshot for the agent
pub fn capture(repo_root: &Path, bench_name: &str) -> Result<TelemetrySnapshot, TelemetryError> {
    let hotspots = capture_cpu_hotspots(repo_root, bench_name)?;
    let allocations = capture_allocation_profile(repo_root, bench_name)?;
    let deltas = capture_benchmark_deltas(repo_root)?;
    let tests = capture_test_summary(repo_root)?;

    Ok(TelemetrySnapshot {
        hotspots,
        allocations,
        benchmark_deltas: deltas,
        test_summary: tests,
    })
}

/// Format telemetry for injection into the agent's working memory.
/// This is the key interface — it turns raw profiling data into
/// natural language the LLM can reason about.
pub fn to_agent_prompt(snapshot: &TelemetrySnapshot) -> String {
    let mut prompt = String::with_capacity(4096);

    prompt.push_str("## 📊 Performance Telemetry\n\n");

    // CPU hotspots
    if !snapshot.hotspots.is_empty() {
        prompt.push_str("### CPU Hotspots (top 10)\n");
        for (i, h) in snapshot.hotspots.iter().take(10).enumerate() {
            prompt.push_str(&format!(
                "{}. `{}` in `{}:{}` — {:.1}% CPU, {} calls, {:.1}µs avg\n",
                i + 1,
                h.function,
                h.file,
                h.line,
                h.cpu_percent,
                h.call_count,
                h.avg_duration_us
            ));
        }
        prompt.push('\n');
    }

    // Allocation hotspots
    if !snapshot.allocations.is_empty() {
        prompt.push_str("### Memory Allocation Hotspots\n");
        for a in snapshot.allocations.iter().take(5) {
            prompt.push_str(&format!(
                "- `{}`: {} allocs/call, {:.1} KB total, {:.1} KB peak live\n",
                a.function,
                a.allocs_per_call,
                a.total_bytes as f64 / 1024.0,
                a.peak_live_bytes as f64 / 1024.0,
            ));
        }
        prompt.push('\n');
    }

    // Benchmark regressions/improvements
    if !snapshot.benchmark_deltas.is_empty() {
        prompt.push_str("### Benchmark Changes vs Baseline\n");
        for d in &snapshot.benchmark_deltas {
            let (icon, direction) = if d.delta_percent > 2.0 {
                ("🔴", "SLOWER")
            } else if d.delta_percent < -2.0 {
                ("🟢", "FASTER")
            } else {
                ("⚪", "STABLE")
            };
            prompt.push_str(&format!(
                "{} `{}`: {:.1}% {} ({:.2}ms → {:.2}ms)\n",
                icon,
                d.name,
                d.delta_percent.abs(),
                direction,
                d.baseline_ms,
                d.current_ms
            ));
        }
        prompt.push('\n');
    }

    // Test summary
    prompt.push_str(&format!(
        "### Test Suite: {}/{} passed ({} failed, {} ignored) in {:.1}s\n",
        snapshot.test_summary.passed,
        snapshot.test_summary.total,
        snapshot.test_summary.failed,
        snapshot.test_summary.ignored,
        snapshot.test_summary.duration.as_secs_f64()
    ));

    prompt
}

fn capture_cpu_hotspots(
    repo_root: &Path,
    bench_name: &str,
) -> Result<Vec<CpuHotspot>, TelemetryError> {
    // Run cargo flamegraph and parse the folded stacks
    let flamegraph_path = repo_root.join("target").join("flamegraph.folded");

    let output = Command::new("cargo")
        .args([
            "flamegraph",
            "--bench",
            bench_name,
            "--output",
            flamegraph_path.to_str().unwrap_or("/dev/null"),
            "--",
            "--bench",
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|e| TelemetryError::ToolFailed("flamegraph".into(), e.to_string()))?;

    // Parse folded stacks into hotspot list
    if flamegraph_path.exists() {
        parse_folded_stacks(&flamegraph_path)
    } else {
        // Fallback: use perf stat or time-based sampling
        Ok(vec![])
    }
}

fn parse_folded_stacks(path: &Path) -> Result<Vec<CpuHotspot>, TelemetryError> {
    let content =
        std::fs::read_to_string(path).map_err(|e| TelemetryError::ParseFailed(e.to_string()))?;

    let mut function_samples: HashMap<String, u64> = HashMap::new();
    let mut total_samples: u64 = 0;

    for line in content.lines() {
        if let Some((stack, count_str)) = line.rsplit_once(' ') {
            if let Ok(count) = count_str.parse::<u64>() {
                total_samples += count;
                // Get the leaf function (last in the stack)
                if let Some(leaf) = stack.split(';').next_back() {
                    *function_samples.entry(leaf.to_string()).or_default() += count;
                }
            }
        }
    }

    let mut hotspots: Vec<CpuHotspot> = function_samples
        .into_iter()
        .map(|(func, samples)| {
            let cpu_percent = if total_samples > 0 {
                (samples as f64 / total_samples as f64) * 100.0
            } else {
                0.0
            };
            CpuHotspot {
                function: func,
                file: String::new(), // Would need DWARF info for this
                line: 0,
                cpu_percent,
                call_count: samples,
                avg_duration_us: 0.0, // Not available from sampling
            }
        })
        .collect();

    hotspots.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap());
    Ok(hotspots)
}

fn capture_allocation_profile(
    _repo_root: &Path,
    _bench_name: &str,
) -> Result<Vec<AllocationHotspot>, TelemetryError> {
    // TODO: Integrate DHAT or a custom global allocator that tracks allocation sites
    // For now, return empty — this is a P2 feature
    Ok(vec![])
}

fn capture_benchmark_deltas(repo_root: &Path) -> Result<Vec<BenchmarkDelta>, TelemetryError> {
    // Criterion stores baselines in target/criterion/
    let criterion_dir = repo_root.join("target").join("criterion");
    if !criterion_dir.exists() {
        return Ok(vec![]);
    }

    let mut deltas = Vec::new();

    // Walk criterion output directories
    if let Ok(entries) = std::fs::read_dir(&criterion_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let estimates = entry.path().join("new").join("estimates.json");
                let baseline = entry.path().join("base").join("estimates.json");

                if estimates.exists() && baseline.exists() {
                    if let (Ok(new_val), Ok(base_val)) = (
                        parse_criterion_estimate(&estimates),
                        parse_criterion_estimate(&baseline),
                    ) {
                        let delta_pct = ((new_val - base_val) / base_val) * 100.0;
                        deltas.push(BenchmarkDelta {
                            name: entry.file_name().to_string_lossy().to_string(),
                            baseline_ms: base_val,
                            current_ms: new_val,
                            delta_percent: delta_pct,
                        });
                    }
                }
            }
        }
    }

    deltas.sort_by(|a, b| {
        b.delta_percent
            .abs()
            .partial_cmp(&a.delta_percent.abs())
            .unwrap()
    });
    Ok(deltas)
}

fn parse_criterion_estimate(path: &Path) -> Result<f64, TelemetryError> {
    let content =
        std::fs::read_to_string(path).map_err(|e| TelemetryError::ParseFailed(e.to_string()))?;
    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| TelemetryError::ParseFailed(e.to_string()))?;

    // Criterion stores mean estimate in nanoseconds
    json["mean"]["point_estimate"]
        .as_f64()
        .map(|ns| ns / 1_000_000.0) // Convert to ms
        .ok_or_else(|| TelemetryError::ParseFailed("No mean estimate found".into()))
}

fn capture_test_summary(repo_root: &Path) -> Result<TestSummary, TelemetryError> {
    let start = std::time::Instant::now();

    let output = Command::new("cargo")
        .args([
            "test",
            "--all-features",
            "--",
            "--format=json",
            "-Z",
            "unstable-options",
        ])
        .current_dir(repo_root)
        .output()
        .map_err(|e| TelemetryError::ToolFailed("cargo test".into(), e.to_string()))?;

    let duration = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut total = 0;
    let mut passed = 0;
    let mut failed = 0;
    let mut ignored = 0;

    for line in stdout.lines() {
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
            if event["type"] == "test" && event["event"].is_string() {
                total += 1;
                match event["event"].as_str() {
                    Some("ok") => passed += 1,
                    Some("failed") => failed += 1,
                    Some("ignored") => ignored += 1,
                    _ => {}
                }
            }
        }
    }

    Ok(TestSummary {
        total,
        passed,
        failed,
        ignored,
        duration,
    })
}

#[derive(Debug)]
pub enum TelemetryError {
    ToolFailed(String, String),
    ParseFailed(String),
}

impl std::fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolFailed(tool, msg) => write!(f, "{} failed: {}", tool, msg),
            Self::ParseFailed(msg) => write!(f, "Parse failed: {}", msg),
        }
    }
}

impl std::error::Error for TelemetryError {}

#[cfg(test)]
mod tests {
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
}
