# Testing Infrastructure & SWE-bench Harness Audit

**Audited files:** `src/testing/`, `src/bench_harness/`, `src/swebench/`, `src/doctor.rs`, `src/llm_doctor.rs`

---

## 1. Stubbed swebench module (`src/swebench/mod.rs`)

### Problem
The root-level `swebench` module is **100% stubbed** and returns fabricated data. It is a trap for any caller expecting real evaluation.

| Line | Issue |
|------|-------|
| 1-8 | Module header explicitly states "STUB — RETURNS MOCK DATA". |
| 94-111 | `SWEBenchEvaluator::load_tasks` returns a single hard-coded task (`"example/repo"`, `"test-001"`). |
| 118-172 | `evaluate_task` **never calls the agent**. It sleeps, builds a fake trajectory, and returns `success: true`, `resolved: true`, `patch_applied: true`. |
| 175-205 | `evaluate_all` iterates over stub tasks and accumulates fake results, producing a meaningless resolution rate. |
| 299-328 | `test_code_generation`, `test_file_editing`, etc. are all no-ops that log a warning and return `Ok(())`. |

### Concrete fix
1. **Delete or deprecate** `src/swebench/mod.rs`. The real implementation lives in `src/bench_harness/swebench_pro/`. Keeping the stub invites accidental use.
2. If backward compatibility is required, re-export `bench_harness::swebench_pro::*` from `src/swebench/mod.rs` and mark the old `SWEBenchEvaluator` as `#[deprecated]`.
3. Update any caller still importing `crate::swebench::SWEBenchEvaluator` to use `crate::bench_harness::swebench_pro::runner::run_swebench_pro`.

---

## 2. Bench harness bugs / missing features

### 2.1 Synchronous SWE-bench runner blocks async runtime
**File:** `src/bench_harness/swebench_pro/runner.rs`  
**Line:** 83 (`pub fn run_swebench_pro`)

The top-level runner is a blocking `fn` that does I/O (cloning repos, spawning processes, writing files). When called from a tokio context it will block the runtime thread for minutes.

**Fix:** Change signature to `pub async fn run_swebench_pro` and use `tokio::task::spawn_blocking` for the CPU-bound / sync parts (git clones, process spawning).

### 2.2 `run_selfware` uses inefficient polling
**File:** `src/bench_harness/swebench_pro/harness.rs`  
**Lines:** 292-333

```rust
loop {
    match child.try_wait()? { ... }
    std::thread::sleep(Duration::from_millis(500));
}
```

Wakes up every 500 ms for the entire agent lifetime. For a 10-minute task that is ~1,200 wasted polls.

**Fix:** Use `tokio::process::Command` with a `tokio::time::timeout` future, or on sync code use `waitpid` with a signal / pipe-based notification.

### 2.3 `probe_endpoint` shells out to `curl`
**File:** `src/bench_harness/swebench_pro/harness.rs`  
**Lines:** 210-223

Spawns a `curl` subprocess for every boot probe. Slow, flaky if `curl` is missing, and hard to debug.

**Fix:** Replace with `reqwest::blocking::get` or `ureq`:
```rust
fn probe_endpoint(port: u16) -> bool {
    ureq::get(&format!("http://127.0.0.1:{}/v1/models", port))
        .timeout(Duration::from_secs(1))
        .call()
        .is_ok()
}
```

### 2.4 `LlamaServer::boot` sleeps for 2 s between probes with no backoff
**File:** `src/bench_harness/swebench_pro/harness.rs`  
**Lines:** 188-193

```rust
while Instant::now() < deadline {
    if probe_endpoint(port) { return Ok(server); }
    std::thread::sleep(Duration::from_secs(2));
}
```

If the server needs 60 s to boot, this probes 30 times. If it needs 5 s, it still probes ~3 times.

**Fix:** Start with a 500 ms sleep and cap at 3 s (exponential backoff). Also log each probe attempt so the user can see progress.

### 2.5 `stop_existing` is a foot-gun
**File:** `src/bench_harness/swebench_pro/harness.rs`  
**Lines:** 137-144

```rust
let _ = Command::new("pkill")
    .args(["-f", "llama-server"])
```

This kills **all** `llama-server` processes on the machine, including ones started by other users or other benchmarks.

**Fix:** Track the PID of the spawned child and only kill that PID family:
```rust
// After spawn
let pid = child.id();
// On teardown
let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
```

### 2.6 No retry / backoff for failed LLM requests in generic harness
**File:** `src/bench_harness/runner.rs`  
**Lines:** 156-172

If the HTTP POST fails (transient network error, 502 from vLLM), the task is immediately marked failed with no retry.

**Fix:** Wrap `client.post(...).send()` in a small retry loop (e.g., 3 attempts with exponential backoff) for 5xx / timeout errors before giving up.

### 2.7 Generic harness spawns all tasks eagerly
**File:** `src/bench_harness/runner.rs`  
**Lines:** 63-81

```rust
for task in tasks {
    let handle = tokio::spawn(async move { ... });
    handles.push(handle);
}
```

With 1,000 tasks this creates 1,000 tokio tasks immediately. They are bounded by the semaphore, but the task objects themselves consume memory and scheduler overhead.

**Fix:** Use `futures::stream::iter(tasks).buffer_unordered(config.max_concurrent)` instead of eager spawning.

### 2.8 `HarnessRunner` does not use streaming completions
**File:** `src/bench_harness/runner.rs`  
**Lines:** 138-144

The request body hard-codes `"stream": false`. For long responses this inflates latency numbers and buffers the full response in memory.

**Fix:** Add a `stream: bool` field to `HarnessConfig`. When enabled, use SSE parsing and update `completion_tokens` as chunks arrive.

### 2.9 `is_test_only_patch` has false positives
**File:** `src/bench_harness/swebench_pro/runner.rs`  
**Lines:** 524-550

```rust
if !path.contains("test") && !basename.contains("test") {
    return false;
}
```

A file named `contest.py` or `testimony.rs` would be misclassified as a test file.

**Fix:** Use regex or stricter matching:
```rust
let is_test_file = |p: &str| {
    let b = p.rsplit('/').next().unwrap_or(p);
    b.starts_with("test_") || b.ends_with("_test.py") || b.ends_with(".test.js")
        || p.contains("/tests/") || p.contains("/test/")
};
```

---

## 3. Verification gate flaws

### 3.1 Non-Rust QA is skipped when Rust files are present
**File:** `src/testing/verification.rs`  
**Lines:** 356-394

```rust
if !rust_files_changed {
    // run Python / Node / Go QA
}
```

If a commit changes both `src/main.rs` and `script.py`, the Python QA is silently skipped. The gate should run language-specific QA for **all** changed languages, not just one.

**Fix:** Remove the `if !rust_files_changed` guard and instead iterate over all distinct languages detected in the changed file list.

### 3.2 `run_cargo_check` misses test / bench targets
**File:** `src/testing/verification.rs`  
**Lines:** 450-455

```rust
Command::new("cargo").arg("check").arg("--message-format=json")
```

No `--all-targets` or `--all-features`. Compilation errors in `tests/`, `benches/`, or behind feature flags are invisible to the gate.

**Fix:** Add `.args(["--all-targets", "--all-features"])` (or make it configurable).

### 3.3 `parse_test_failures` is extremely brittle
**File:** `src/testing/verification.rs`  
**Lines:** 748-787

```rust
if line.contains("FAILED") && line.contains("test ") {
    let test_name = line.split("test ").nth(1) ...
}
```

- Breaks on test names containing `"test "` (e.g., `test_test_foo`).
- Does not parse `cargo test --message-format=json` output, which is far more structured.
- Misses `should_panic` tests that fail to panic.
- Does not capture the actual assertion message.

**Fix:** Run `cargo test --message-format=json` and parse the JSON `test` / `suite` events. This gives exact test names, durations, and stdout capture.

### 3.4 Side-effect detection is useless
**File:** `src/testing/verification.rs`  
**Lines:** 671-705

```rust
async fn detect_side_effects(&self, files: &[String]) -> Vec<SideEffect> {
    for file in files {
        let path = self.project_root.join(file);
        if path.exists() {
            effects.push(SideEffect { effect_type: FileModified, ... });
        }
    }
}
```

It has **no baseline**. It cannot distinguish "FileCreated" from "FileModified" because it never recorded the pre-change state. It also never detects `FileDeleted`.

**Fix:** Before running checks, snapshot the file list (e.g., `git status --porcelain` or a hash map of `std::fs::read_dir`). Compare post-check to pre-check.

### 3.5 File hash cache reads entire file into memory
**File:** `src/testing/verification.rs`  
**Lines:** 210-223

```rust
let mut contents = Vec::new();
file.read_to_end(&mut contents).ok()?;
let mut hasher = DefaultHasher::new();
hasher.write(&contents);
```

For a 100 MB log file this allocates 100 MB.

**Fix:** Stream the hash:
```rust
use std::io::BufReader;
let mut reader = BufReader::new(file);
let mut hasher = DefaultHasher::new();
std::io::copy(&mut reader, &mut hasher).ok()?;
```
(Note: `std::hash::Hasher` does not implement `Write`, so use `blake3` or `sha2` which do, or manually hash 8 KB chunks.)

### 3.6 `full_verify` passes empty file list, skipping all checks
**File:** `src/testing/verification.rs`  
**Lines:** 442-444

```rust
pub async fn full_verify(&mut self) -> Result<VerificationReport> {
    self.verify_change(&[], "full_verification").await
}
```

Because `verify_change` early-returns when `files_to_check.is_empty()`, `full_verify` never runs any checks.

**Fix:** Add a special-case bypass in `verify_change` when `trigger == "full_verification"` to run all checks regardless of file list.

---

## 4. Test infrastructure gaps

### 4.1 Contract testing stubs don't bind to real ports
**File:** `src/testing/contract_testing/stubs.rs`  
**Lines:** 217-223

```rust
pub fn start(&mut self) {
    self.running = true;
}
```

`MockServer` is a pure in-memory struct. There is no TCP server. Integration tests cannot verify real HTTP client behavior against it.

**Fix:** Either integrate `wiremock` (a real HTTP mock server) or rename `MockServer` to `StubRegistry` to avoid confusion.

### 4.2 API testing framework has no HTTP client
**File:** `src/testing/api_testing.rs`  
**Line:** 10

```rust
#![allow(dead_code, unused_imports, unused_variables)]
```

The entire file defines request/response types and an `ApiTestClient`, but `ApiTestClient` has **no method that makes an actual HTTP request**. It only records pre-built responses (`run_test` takes a `HttpResponse` argument).

**Fix:** Implement `ApiTestClient::execute(&self, request: &HttpRequest) -> Result<HttpResponse>` using `reqwest` or `hyper`, with proper timeout and redirect handling.

### 4.3 Code review module is dead code
**File:** `src/testing/code_review.rs`  
**Line:** 1

```rust
//! Code Review Assistant (EXPERIMENTAL — no call sites, candidate for removal)
```

2,656 lines of code with zero production callers. It increases compile times and maintenance burden.

**Fix:** Remove the module. If needed later, restore it from git history.

### 4.4 No snapshot testing
There is no `insta` or similar snapshot testing anywhere in the codebase. Complex reports (`HarnessReport`, `VerificationReport`, `EvaluationReport`) are tested with ad-hoc `assert!(json.contains(...))` checks that are brittle and don't catch formatting regressions.

**Fix:** Add `insta` to dev-dependencies and snapshot the JSON output of key report types.

### 4.5 No property-based or fuzz testing
No `proptest`, `quickcheck`, or `cargo-fuzz` integration. The hash functions, diff parsers, and JSON extractors would benefit from fuzzing.

### 4.6 `language_qa` tests don't test actual detection
**File:** `src/testing/language_qa.rs`  
**Lines:** 545-550

```rust
fn test_qa_language_detect() {
    let tmp = std::env::temp_dir().join("nonexistent_qa_test");
    let lang = QaLanguage::detect(&tmp);
    assert_eq!(lang, QaLanguage::Unknown);
}
```

Only tests the negative case. Does not create temp dirs with `Cargo.toml`, `package.json`, etc.

**Fix:** Use `tempfile::TempDir` to create real directories and assert detection works.

### 4.7 Doctor lacks GPU / CUDA checks
**File:** `src/doctor.rs`

The doctor checks for `rustc`, `git`, `docker`, etc., but never checks for:
- `nvidia-smi` / CUDA availability
- `llama-server` binary existence (critical for SWE-bench)
- Python `datasets` library (critical for SWE-bench dataset loading)

**Fix:** Add a `Category::GpuCompute` and `Category::SwebenchDeps` section to the doctor.

---

## 5. Performance issues in the harness

### 5.1 llama-server boot is fully synchronous
**File:** `src/bench_harness/swebench_pro/harness.rs`  
**Lines:** 148-198

`LlamaServer::boot` is a blocking `fn` that does `Command::spawn`, then loops with `std::thread::sleep`. If this is called on a single-threaded tokio runtime, all async tasks freeze for the boot duration (up to 180 s).

**Fix:** Provide `LlamaServer::boot_async` that uses `tokio::process::Command` and `tokio::time::interval` for probing.

### 5.2 Parallel trial execution uses mutex-protected VecDeque
**File:** `src/bench_harness/swebench_pro/runner.rs`  
**Lines:** 279-310

```rust
let queue: Arc<Mutex<Vec<usize>>> = ...;
// Each worker:
let idx = { queue.lock().unwrap().pop() };
```

With high concurrency this creates lock contention on every task dequeue.

**Fix:** Use `crossbeam::deque::Worker` / `Stealer` or a `std::sync::mpsc` channel. Pre-send indices into the channel; workers simply `recv()`.

### 5.3 Dataset is re-downloaded on every run
**File:** `src/bench_harness/swebench_pro/dataset.rs`  
**Lines:** 36-46

```rust
const LOADER_PY: &str = r#"
    from datasets import load_dataset
    ds = load_dataset("ScaleAI/SWE-bench_Pro", split="test")
"#;
```

The Python `datasets` library caches by default, but the inline script does not set `cache_dir` explicitly. If the HF cache is missing or corrupted, the harness re-downloads ~GBs of data.

**Fix:** Explicitly set `cache_dir` to a known location (e.g., `opts.output.join(".hf_cache")`) and log the cache path.

### 5.4 `capture_patch` runs `git add -A` unconditionally
**File:** `src/bench_harness/swebench_pro/harness.rs`  
**Lines:** 354-360

```rust
let _ = Command::new("git").args(["add", "-A"]).status();
```

This mutates the workdir index. If the harness crashes before `git reset`, the repo is left in a dirty state.

**Fix:** Use `git diff` without staging, or stage to a temporary index file via `GIT_INDEX_FILE`.

---

## 6. Blockers for real SWE-bench evaluation

| # | Blocker | Location | Severity |
|---|---------|----------|----------|
| 1 | **Stubbed swebench module** returns fake results | `src/swebench/mod.rs` | 🔴 Critical |
| 2 | **No real agent execution** in `evaluate_task` | `src/swebench/mod.rs:118-172` | 🔴 Critical |
| 3 | **User-specific hardcoded paths** for llama-server binary & models | `src/bench_harness/swebench_pro/harness.rs:47-75` | 🟠 High |
| 4 | **Synchronous runner** blocks async runtime | `src/bench_harness/swebench_pro/runner.rs:83` | 🟠 High |
| 5 | **Requires Python `datasets`** with no fallback | `src/bench_harness/swebench_pro/dataset.rs:36-46` | 🟠 High |
| 6 | **No Docker health check** before official eval | `src/bench_harness/swebench_pro/runner.rs:817-834` | 🟡 Medium |
| 7 | **Hardcoded quant catalog** with relative paths | `src/bench_harness/swebench_pro/catalog.rs:19-87` | 🟡 Medium |
| 8 | **`pkill -f llama-server`** can kill other users' processes | `src/bench_harness/swebench_pro/harness.rs:137-144` | 🟡 Medium |
| 9 | **No resume / checkpointing** beyond `skip_existing` | `src/bench_harness/swebench_pro/runner.rs:380-407` | 🟡 Medium |
| 10 | **Test-only patch detection is naive** | `src/bench_harness/swebench_pro/runner.rs:524-550` | 🟢 Low |

### Recommended priority order to unblock evaluation

1. **Remove the stub** (`src/swebench/mod.rs`) so nobody accidentally uses it.
2. **Make `run_swebench_pro` async** and call it via `tokio::task::spawn_blocking` from the CLI.
3. **Replace hardcoded paths** with env-var fallbacks that have sensible defaults (e.g., `/usr/local/bin/llama-server`, `./models`).
4. **Add a `--dataset-cache` flag** and set `cache_dir` in the Python loader.
5. **Add Docker check** in `run_official_eval` before spawning the evaluator:
   ```rust
   if Command::new("docker").arg("info").status()?.success() == false {
       bail!("Docker is not running — official eval requires Docker");
   }
   ```
6. **Track child PID** and replace `pkill` with targeted `kill`.

---

*End of audit.*
