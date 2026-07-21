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
    repo_root: &Path,
    bench_name: &str,
) -> Result<Vec<AllocationHotspot>, TelemetryError> {
    // Strategy 1: Look for DHAT output produced by running the benchmark
    // under `dhat` (e.g. `cargo bench --features dhat`).
    if let Some(hotspots) = try_parse_dhat_output(repo_root)? {
        return Ok(hotspots);
    }

    // Strategy 2: Fall back to the lightweight tracking allocator that
    // is always available (no external tool required).
    let stats = global_alloc_stats();
    if stats.total_allocs() > 0 {
        let live = stats.current_live_bytes();
        // Update peak if current exceeds it.
        stats.update_peak(live);
        return Ok(vec![AllocationHotspot {
            function: "<global>".to_string(),
            allocs_per_call: stats.total_allocs(),
            total_bytes: stats.total_bytes_allocated(),
            peak_live_bytes: stats.peak_live_bytes(),
        }]);
    }

    // No allocation data available — nothing was profiled.
    Ok(vec![])
}

// ---------------------------------------------------------------------------
// DHAT output parsing
// ---------------------------------------------------------------------------

/// Attempt to read and parse a DHAT JSON profile from the repository.
/// DHAT writes files like `dhat-heap.json` or `dhat.out` when the program
/// is run under the `dhat` heap profiler.
fn try_parse_dhat_output(
    repo_root: &Path,
) -> Result<Option<Vec<AllocationHotspot>>, TelemetryError> {
    // Common DHAT output locations
    let candidates = [
        repo_root.join("dhat.out"),
        repo_root.join("dhat-heap.json"),
        repo_root.join("target").join("dhat.out"),
        repo_root.join("target").join("dhat-heap.json"),
    ];

    for path in &candidates {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Some(hotspots) = parse_dhat_json(&content) {
                    return Ok(Some(hotspots));
                }
            }
        }
    }

    Ok(None)
}

/// Parse a DHAT JSON profile into allocation hotspots.
///
/// DHAT's JSON format includes a `"heap_stats"` object and a `"alloc_fns"`
/// array. We extract per-function allocation counts and byte totals.
fn parse_dhat_json(content: &str) -> Option<Vec<AllocationHotspot>> {
    // DHAT JSON is a single object. We do a lightweight parse without
    // pulling in a JSON dependency — extract fields via simple string search.
    let mut hotspots = Vec::new();

    // Look for the "alloc_fns" array entries of the form:
    // { "fns": [{ "n": <count>, "tb": <total_bytes>, "pb": <peak_bytes>, ... }] }
    // Each function block is a JSON object with descriptive fields.
    for line in content.lines() {
        let trimmed = line.trim();
        // DHAT groups entries; we look for lines containing allocation
        // function names and extract numeric fields heuristically.
        if let Some(func) = extract_json_string_field(trimmed, "desc") {
            let allocs = extract_json_uint_field(trimmed, "n").unwrap_or(0);
            let total_bytes = extract_json_uint_field(trimmed, "tb").unwrap_or(0);
            let peak_bytes = extract_json_uint_field(trimmed, "pb").unwrap_or(0);

            if allocs > 0 || total_bytes > 0 {
                hotspots.push(AllocationHotspot {
                    function: func,
                    allocs_per_call: allocs,
                    total_bytes,
                    peak_live_bytes: peak_bytes,
                });
            }
        }
    }

    if hotspots.is_empty() {
        None
    } else {
        // Sort by total bytes descending
        hotspots.sort_by_key(|b| std::cmp::Reverse(b.total_bytes));
        Some(hotspots)
    }
}

fn extract_json_string_field(line: &str, field: &str) -> Option<String> {
    let needle = format!("\"{}\"", field);
    let idx = line.find(&needle)?;
    let rest = &line[idx + needle.len()..];
    let colon = rest.find(':')?;
    let after = &rest[colon + 1..];
    let quote_start = after.find('"')?;
    let value_start = &after[quote_start + 1..];
    let quote_end = value_start.find('"')?;
    Some(value_start[..quote_end].to_string())
}

fn extract_json_uint_field(line: &str, field: &str) -> Option<u64> {
    let needle = format!("\"{}\"", field);
    let idx = line.find(&needle)?;
    let rest = &line[idx + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let end = after
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after.len());
    after[..end].parse::<u64>().ok()
}

// ---------------------------------------------------------------------------
// Lightweight tracking allocator (no external dependencies)
// ---------------------------------------------------------------------------

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

/// Global allocation statistics tracked by `TrackingAllocator`.
/// A single static instance is shared so that any code path can read
/// cumulative allocation data without coordination.
static GLOBAL_ALLOC_STATS: AllocStats = AllocStats::new();

/// A wrapper around the system allocator that counts allocations,
/// deallocations, and tracks live/peak bytes.
///
/// To enable tracking, set this as the global allocator:
/// ```ignore
/// #[global_allocator]
/// static GLOBAL: TrackingAllocator = TrackingAllocator::system();
/// ```
pub struct TrackingAllocator<A: GlobalAlloc = System> {
    inner: A,
}

impl TrackingAllocator<System> {
    pub const fn system() -> Self {
        TrackingAllocator { inner: System }
    }
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for TrackingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = self.inner.alloc(layout);
        if !ptr.is_null() {
            GLOBAL_ALLOC_STATS.record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        GLOBAL_ALLOC_STATS.record_dealloc(layout.size());
        self.inner.dealloc(ptr, layout);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = self.inner.alloc_zeroed(layout);
        if !ptr.is_null() {
            GLOBAL_ALLOC_STATS.record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Treat realloc as dealloc + alloc for accounting simplicity
        GLOBAL_ALLOC_STATS.record_dealloc(layout.size());
        let new_ptr = self.inner.realloc(ptr, layout, new_size);
        if !new_ptr.is_null() {
            GLOBAL_ALLOC_STATS.record_alloc(new_size);
        }
        new_ptr
    }
}

/// Cumulative allocation statistics, updated atomically.
pub struct AllocStats {
    total_allocs: AtomicU64,
    total_deallocs: AtomicU64,
    total_bytes_allocated: AtomicU64,
    total_bytes_deallocated: AtomicU64,
    current_live_bytes: AtomicU64,
    peak_live_bytes: AtomicU64,
}

impl AllocStats {
    const fn new() -> Self {
        AllocStats {
            total_allocs: AtomicU64::new(0),
            total_deallocs: AtomicU64::new(0),
            total_bytes_allocated: AtomicU64::new(0),
            total_bytes_deallocated: AtomicU64::new(0),
            current_live_bytes: AtomicU64::new(0),
            peak_live_bytes: AtomicU64::new(0),
        }
    }

    fn record_alloc(&self, size: usize) {
        self.total_allocs.fetch_add(1, Ordering::Relaxed);
        self.total_bytes_allocated
            .fetch_add(size as u64, Ordering::Relaxed);
        let live = self
            .current_live_bytes
            .fetch_add(size as u64, Ordering::Relaxed)
            + size as u64;
        self.update_peak(live);
    }

    fn record_dealloc(&self, size: usize) {
        self.total_deallocs.fetch_add(1, Ordering::Relaxed);
        self.total_bytes_deallocated
            .fetch_add(size as u64, Ordering::Relaxed);
        self.current_live_bytes
            .fetch_sub(size as u64, Ordering::Relaxed);
    }

    fn update_peak(&self, live: u64) {
        let mut current_peak = self.peak_live_bytes.load(Ordering::Relaxed);
        while live > current_peak {
            match self.peak_live_bytes.compare_exchange_weak(
                current_peak,
                live,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_peak = actual,
            }
        }
    }

    /// Access the global allocation statistics.
    pub fn total_allocs(&self) -> u64 {
        self.total_allocs.load(Ordering::Relaxed)
    }
    pub fn total_deallocs(&self) -> u64 {
        self.total_deallocs.load(Ordering::Relaxed)
    }
    pub fn total_bytes_allocated(&self) -> u64 {
        self.total_bytes_allocated.load(Ordering::Relaxed)
    }
    pub fn total_bytes_deallocated(&self) -> u64 {
        self.total_bytes_deallocated.load(Ordering::Relaxed)
    }
    pub fn current_live_bytes(&self) -> u64 {
        self.current_live_bytes.load(Ordering::Relaxed)
    }
    pub fn peak_live_bytes(&self) -> u64 {
        self.peak_live_bytes.load(Ordering::Relaxed)
    }
}

/// Retrieve a reference to the global allocation statistics.
pub fn global_alloc_stats() -> &'static AllocStats {
    &GLOBAL_ALLOC_STATS
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
#[path = "../../tests/unit/evolution/telemetry/telemetry_test.rs"]
mod tests;
