# Benchmark Review and OpenRouter Run Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Review the existing Selfware SWE-bench Pro harness results, then run the Project E2E and quant benchmarks against OpenRouter models to compare agentic and single-turn coding performance.

**Architecture:** Use the existing harnesses unchanged where possible (`system_tests/projecte2e/run_projecte2e.sh`, `examples/quant_benchmark.rs`, and `examples/multilang_bench.rs`). OpenRouter authentication is injected via the `SELFWARE_API_KEY` environment variable and per-model TOML configs already under `system_tests/projecte2e/config/`. The built-in `bench_harness` examples do not send an `Authorization` header, so they require a small env-to-header patch for OpenRouter.

**Tech Stack:** Rust (`cargo`), OpenRouter API, `selfware` release binary, bash, Python harness utilities.

---

### Task 1: Review existing SWE-bench Pro results

**Files:**
- Read: `system_tests/swe_bench_pro/sweap_results_snapshot.md`
- Read: `system_tests/swe_bench_pro/benchmark_leaderboard.md`
- Read: `system_tests/swe_bench_pro/openrouter_pilot_final_report.md`
- Read: `system_tests/swe_bench_pro/harness_failure_analysis.md`

- [ ] **Step 1: Summarize the current leaderboard**

Run:
```bash
cat system_tests/swe_bench_pro/benchmark_leaderboard.md
```
Expected: Markdown table showing pass rates 0–8.3% on SWE-bench Pro sample_50; top models are `gpt-5-mini` (8.3%), `glm-5.2-nitro` (6.5%), `gemini-3.5-flash` (2.2%).

- [ ] **Step 2: Read the failure analysis**

Run:
```bash
cat system_tests/swe_bench_pro/harness_failure_analysis.md
```
Expected: Two-page document explaining that most runs produce empty/invalid patches and the evaluation entryscript silently runs tests on the unpatched base commit, causing ~100% pass-to-pass and ~0% fail-to-pass.

- [ ] **Step 3: Record the take-away**

Append a one-line summary to the plan file or a scratch note:
```markdown
- Full SWE-bench Pro harness is currently measuring patch-generation + patch-application, not true fix correctness; pilot on first 10 instances showed 12–57% pass rates.
```

---

### Task 2: Verify build, API key, and OpenRouter config

**Files:**
- Read: `system_tests/projecte2e/config/openrouter.example.toml`
- Read: `system_tests/projecte2e/config/openrouter_<model>.toml` (chosen in Step 3)
- Modify: none (env-only)

- [ ] **Step 1: Confirm the OpenRouter API key is exported**

Run:
```bash
[[ -n "$SELFWARE_API_KEY" ]] && echo "key set" || echo "key NOT set"
```
Expected: `key set`. If `key NOT set`, stop and ask the user for the key.

- [ ] **Step 2: Build the release selfware binary**

Run:
```bash
cargo build --release --all-features -q
ls -lh target/release/selfware
```
Expected: Binary exists and is executable (~tens of MB).

- [ ] **Step 3: Pick a model profile and test endpoint connectivity**

Choose a recommended medium-tier model from `system_tests/projecte2e/config/openrouter_models.toml` (e.g. `glm-5.2-nitro` or `gemini-3.5-flash`). Test:
```bash
export SELFWARE_API_KEY="$SELFWARE_API_KEY"
export CONFIG="system_tests/projecte2e/config/openrouter_glm-5.2-nitro.toml"
curl -fsS -H "Authorization: Bearer $SELFWARE_API_KEY" \
  "$(grep '^endpoint' "$CONFIG" | head -1 | sed 's/.*= *"//;s/".*//')/models" \
  | head -c 200
```
Expected: JSON listing of available models, no HTTP 401/403.

---

### Task 3: Run Project E2E benchmark via OpenRouter

**Files:**
- Use: `system_tests/projecte2e/run_projecte2e.sh`
- Use: `system_tests/projecte2e/config/openrouter_<model>.toml`

- [ ] **Step 1: Run the full Project E2E suite with the chosen model**

Run:
```bash
export SELFWARE_API_KEY="$SELFWARE_API_KEY"
export CONFIG_FILE="system_tests/projecte2e/config/openrouter_glm-5.2-nitro.toml"
export TIMEOUT_MULTIPLIER=2
./system_tests/projecte2e/run_projecte2e.sh
```
Expected: Script builds `selfware`, runs six coding scenarios + one swarm scenario, and writes `system_tests/projecte2e/reports/<timestamp>/summary.md`. Allow 15–30 minutes.

- [ ] **Step 2: Capture the Project E2E summary**

Run:
```bash
cat system_tests/projecte2e/reports/latest/summary.md
```
Expected: Markdown report with per-scenario scores and an overall rating (Poor/Fair/Good/Excellent).

- [ ] **Step 3: Copy the report to a dated result directory**

Run:
```bash
mkdir -p benchmark_results/2026-06-24
cp system_tests/projecte2e/reports/latest/summary.md \
   benchmark_results/2026-06-24/projecte2e_glm-5.2-nitro_summary.md
cp system_tests/projecte2e/reports/latest/results.tsv \
   benchmark_results/2026-06-24/projecte2e_glm-5.2-nitro_results.tsv 2>/dev/null || true
```
Expected: Files copied without error.

---

### Task 4: Patch `quant_benchmark` to read the API key from the environment

**Files:**
- Modify: `examples/quant_benchmark.rs:564–572`

The `quant_benchmark` example constructs a `Config` directly but does not set `api_key`, so OpenRouter rejects unauthenticated requests.

- [ ] **Step 1: Add API key load to the speed-probe config**

Replace:
```rust
let cfg = Config {
    endpoint: args.endpoint.clone(),
    model: args.model.clone(),
    max_tokens: 200,
    temperature: 0.0,
    context_length: 32768,
    ..Config::default()
};
```
with:
```rust
let api_key = std::env::var("SELFWARE_API_KEY").ok().filter(|s| !s.is_empty());
let cfg = Config {
    endpoint: args.endpoint.clone(),
    model: args.model.clone(),
    api_key: api_key.map(|s| s.into()),
    max_tokens: 200,
    temperature: 0.0,
    context_length: 32768,
    ..Config::default()
};
```

- [ ] **Step 2: Verify the change compiles**

Run:
```bash
cargo check --release --example quant_benchmark
```
Expected: `Finished` with no errors.

---

### Task 5: Run `quant_benchmark` against OpenRouter

**Files:**
- Use: `examples/quant_benchmark.rs`
- Output: `benchmark_results/2026-06-24/quant_<model>.json`

- [ ] **Step 1: Run a subset of SAB-style scenarios via OpenRouter**

Run:
```bash
export SELFWARE_API_KEY="$SELFWARE_API_KEY"
cargo run --release --example quant_benchmark -- \
  --endpoint https://openrouter.ai/api/v1 \
  --quant glm-5.2-nitro \
  --model z-ai/glm-5.2:nitro \
  --output benchmark_results/2026-06-24/quant_glm-5.2-nitro.json
```
Expected: Console logs each scenario (easy_calculator, easy_string_ops, medium_bitset, medium_json_merge, actor_pdvr, hard_event_bus, hard_scheduler, unsafe_scanner, viz_ascii_table, viz_maze_gen, viz_svg_chart) with pre-fail, agent exit, post-pass, wall time, and steps. Allow 20–40 minutes.

- [ ] **Step 2: Capture the markdown summary printed to stderr**

Run the command above with stderr redirected:
```bash
cargo run --release --example quant_benchmark -- \
  --endpoint https://openrouter.ai/api/v1 \
  --quant glm-5.2-nitro \
  --model z-ai/glm-5.2:nitro \
  --output benchmark_results/2026-06-24/quant_glm-5.2-nitro.json \
  2> benchmark_results/2026-06-24/quant_glm-5.2-nitro.log
```
Expected: `benchmark_results/2026-06-24/quant_glm-5.2-nitro.log` contains the markdown table.

---

### Task 6: (Optional) Run `multilang_bench` against OpenRouter

**Files:**
- Modify: `examples/multilang_bench.rs:174–184` (add API key support)
- Use: `examples/multilang_bench.rs`

The built-in `bench_harness` `HarnessRunner` does not send an `Authorization` header. For OpenRouter we must either run through a local authenticated proxy or patch the example to pass a header. The simplest route is to skip this unless a fast surface benchmark is specifically wanted.

- [ ] **Step 1: Decide whether to run `multilang_bench`**

If the user only wants agentic/repo-level benchmarks, skip this task. If a single-turn multi-language surface benchmark is wanted, patch `HarnessConfig` to carry an `api_key` field and update `src/bench_harness/runner.rs` to send it.

---

### Task 7: Aggregate and report results

**Files:**
- Create: `benchmark_results/2026-06-24/comparison_report.md`

- [ ] **Step 1: Collect result artifacts**

Run:
```bash
ls -1 benchmark_results/2026-06-24/
```
Expected: `projecte2e_glm-5.2-nitro_summary.md`, `projecte2e_glm-5.2-nitro_results.tsv`, `quant_glm-5.2-nitro.json`, `quant_glm-5.2-nitro.log`.

- [ ] **Step 2: Write a combined comparison report**

Create `benchmark_results/2026-06-24/comparison_report.md` with:
```markdown
# Benchmark Comparison Report — 2026-06-24

## Existing SWE-bench Pro (Selfware harness)
- Top pass rate on sample_50: GPT-5-mini 8.3%, GLM-5.2-nitro 6.5%, Gemini-3.5-flash 2.2%.
- Harness failure analysis: most runs produce empty/invalid patches; evaluation entryscript silently runs tests on unpatched base commit.
- Pilot on first 10 instances: GLM-5.2 57.1%, Gemini-3.5 Flash 44.4%.

## Project E2E (agentic, OpenRouter)
- Overall score and rating from `projecte2e_glm-5.2-nitro_summary.md`.
- Per-scenario pass/fail from `projecte2e_glm-5.2-nitro_results.tsv`.

## Quant Benchmark (SAB-style agentic scenarios, OpenRouter)
- Scenarios passed / total from `quant_glm-5.2-nitro.json`.
- Speed probe tok/s from `quant_glm-5.2-nitro.log`.

## Interpretation
- Compare Project E2E and Quant results to identify whether failures are in patch generation, tool use, or test verification.
- Note any scenario that passes in one harness but fails in the other.
```

- [ ] **Step 3: Print the final comparison to the user**

Run:
```bash
cat benchmark_results/2026-06-24/comparison_report.md
```
Expected: The rendered markdown report is shown.

---

## Self-Review

1. **Spec coverage:**
   - Review existing SWE-bench Pro results → Task 1.
   - Run other benchmarks using OpenRouter key → Tasks 2–5.
   - Verify and report outcomes → Tasks 6–7.
2. **Placeholder scan:** No TBD/TODO/fill-in-details remain.
3. **Type consistency:** `Config.api_key` is a `String`/`SecretString` wrapper; the example uses `api_key: api_key.map(|s| s.into())` which matches existing `Config` construction patterns.
