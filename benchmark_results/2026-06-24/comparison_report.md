# Benchmark Comparison Report — 2026-06-24

## Models evaluated

| Model | Endpoint | Harnesses |
|-------|----------|-----------|
| `z-ai/glm-5.2:nitro` | OpenRouter | Project E2E (partial), quant_benchmark |
| `google/gemini-3.5-flash:nitro` | OpenRouter | Project E2E (full), quant_benchmark |

## Existing SWE-bench Pro (Selfware harness)

Leaderboard snapshot (`system_tests/swe_bench_pro/benchmark_leaderboard.md`):

| Rank | Model | Sample | Pass | Completed | Pass Rate |
|------|-------|--------|------|-----------|------------|
| 1 | gpt-5-mini | 50 | 1 | 12 | 8.3% |
| 2 | glm-5.2-nitro | 50 | 3 | 46 | 6.5% |
| 3 | gemini-3.5-flash | 50 | 1 | 45 | 2.2% |
| 4 | gemini-3.5-flash-nitro | 50 | 1 | 46 | 2.2% |

**Key finding from `harness_failure_analysis.md`:** The dominant failure mode is not incorrect fixes. The evaluation entryscript silently continues when `git apply` fails, so tests often run on the unpatched base commit. Combined with a high rate of empty predictions, the full SWE-bench Pro numbers currently measure patch-generation + patch-application hygiene more than true fix correctness.

Pilot on the first 10 instances (`openrouter_pilot_final_report.md`) showed much higher pass rates:
- `z-ai/glm-5.2`: 57.1%
- `google/gemini-3.5-flash_opt`: 44.4%
- `moonshotai/kimi-k2.6`: 37.5%

This confirms the harness/data pipeline is the main bottleneck on the full sample, not necessarily the models.

## Project E2E (agentic Rust templates, OpenRouter)

### `z-ai/glm-5.2:nitro` — partial run

**Status:** 3 of 7 scenarios completed before the run hung on `medium_bitset`.

| Scenario | Difficulty | Baseline | Post | Agent Exit | Score | Notes |
|----------|------------|----------|------|------------|-------|-------|
| easy_calculator | easy | 0 (pass) | 0 (pass) | 1 | 70 | baseline_already_green |
| easy_string_ops | easy | 0 (pass) | 0 (pass) | 1 | 70 | baseline_already_green |
| medium_json_merge | medium | 0 (pass) | 0 (pass) | 1 | 70 | baseline_already_green |

The `timeout 600` wrapper did not terminate the `selfware` process on `medium_bitset`, so the script was manually stopped after ~35 minutes.

### `google/gemini-3.5-flash:nitro` — full run

**Status:** Completed — overall score **70.0/100**, rating **Good**, coding scenarios **5/6 passed**.

| Scenario | Difficulty | Baseline | Post | Agent Exit | Timed out | Duration (s) | Score | Notes |
|----------|------------|----------|------|------------|-----------|--------------|-------|-------|
| easy_calculator | easy | 0 | 0 | 1 | no | 77 | 70 | baseline_already_green |
| easy_string_ops | easy | 0 | 0 | 1 | no | 131 | 70 | baseline_already_green |
| medium_json_merge | medium | 0 | 0 | 1 | no | 145 | 70 | baseline_already_green |
| medium_bitset | medium | 0 | 101 | 124 | **yes** | 610 | 0 | baseline_already_green, agent_timeout |
| hard_scheduler | hard | 101 | 0 | 1 | no | 30 | **90** | real fix |
| hard_event_bus | hard | 101 | 0 | 1 | no | 97 | **90** | real fix |
| swarm_session | swarm | n/a | n/a | 0 | no | 12 | 100 | spawned=3 |

**Important caveat:** The easy/medium templates appear to be already fixed (baselines green), so their 70/100 scores do **not** measure bug-fixing ability. The two hard scenarios (`hard_scheduler`, `hard_event_bus`) had broken baselines and were fixed by the agent, scoring 90/100 each.

**Notable failure:** `medium_bitset` hit the scenario timeout on both model runs. It is the same scenario that hung with `glm-5.2-nitro`. With `gemini-3.5-flash:nitro` the `timeout` wrapper did eventually kill it and the suite continued.

## Quant Benchmark (SAB-style agentic scenarios, OpenRouter)

### `z-ai/glm-5.2:nitro`

**Overall:** 0/11 scenarios passed. **Speed:** 112.0 tok/s median.

Every scenario aborted with `READ_LOOP_NO_EDIT` at step 11. The agent repeatedly ran `cargo test` but never called `file_edit`.

### `google/gemini-3.5-flash:nitro`

**Overall:** 0/11 scenarios passed. **Speed:** 145.0 tok/s median.

Same `nonzero(5)` / read-loop outcome, but the failure mode is more varied:
- The agent repeatedly tries an unregistered `tool_search` tool, which is blocked by the safety checker.
- It then falls back to `shell_exec cargo test` and `file_read`, but still rarely edits.
- `viz_ascii_table` reached 16 steps (vs. 11 for all others), suggesting slightly better persistence, but still no pass.
- Near-misses: `unsafe_scanner` 17/20 tests passing, `medium_bitset` 11/14 tests passing.

| Scenario | Post-pass | Steps | Validator Summary |
|----------|-----------|------:|-------------------|
| easy_calculator | ✗ | 11 | 2 passed; 1 failed |
| easy_string_ops | ✗ | 11 | 3 passed; 1 failed |
| medium_bitset | ✗ | 11 | 11 passed; 3 failed |
| medium_json_merge | ✗ | 11 | 2 passed; 1 failed |
| actor_pdvr | ✗ | 11 | 5 passed; 14 failed |
| hard_event_bus | ✗ | 11 | 2 passed; 5 failed |
| hard_scheduler | ✗ | 11 | 0 passed; 4 failed |
| unsafe_scanner | ✗ | 11 | 17 passed; 3 failed |
| viz_ascii_table | ✗ | 16 | 7 passed; 3 failed |
| viz_maze_gen | ✗ | 11 | 9 passed; 5 failed |
| viz_svg_chart | ✗ | 11 | 0 passed; 8 failed |

## Head-to-head comparison

| Metric | `glm-5.2-nitro` | `gemini-3.5-flash:nitro` |
|--------|-----------------|--------------------------|
| Project E2E overall | partial, 3/3 green templates | **70.0/100**, 5/6 coding, 2 real hard-scenario fixes |
| quant_benchmark pass rate | 0/11 | 0/11 |
| quant_benchmark speed | 112 tok/s | **145 tok/s** |
| quantBenchmark near-misses | few | unsafe_scanner 17/20, medium_bitset 11/14 |
| Agent behavior | cargo-test loop | tries blocked `tool_search`, then cargo-test loop |

## Interpretation

1. **The harnesses are currently the signal, not the models.**
   - SWE-bench Pro is dominated by patch-application and empty-patch failures.
   - Project E2E easy/medium templates are no longer bugged, so they cannot measure fix ability until re-corrupted.
   - quant_benchmark correctly injects bugs but the agent loop stalls before editing (read-loop / no-edit termination).

2. **`gemini-3.5-flash:nitro` is clearly more capable than `glm-5.2:nitro` in this agent loop.**
   - It completed the full Project E2E suite and fixed two hard scenarios.
   - It is ~30% faster on the speed probe.
   - It got closer to passing several quant_benchmark scenarios.

3. **Both models are bottlenecked by the same agent/harness issues in quant_benchmark.**
   - `glm-5.2:nitro` never makes an edit.
   - `gemini-3.5-flash:nitro` attempts an unregistered `tool_search`, wastes steps on safety errors, and still rarely edits.

4. **Recommended next steps before another run:**
   - Fix the `tool_search` issue: either register the tool in the safety checker or remove it from the model's tool catalog/prompt.
   - Add an explicit "you must call file_edit" directive and an early-edit deadline to the quant_benchmark prompt.
   - Re-inject bugs into the Project E2E easy/medium templates so baseline tests fail.
   - Consider raising `max_iterations` or the `medium_bitset` timeout, since that scenario consistently overruns.

## Artifacts

- `benchmark_results/2026-06-24/comparison_report.md`
- `benchmark_results/2026-06-24/quant_glm-5.2-nitro.json` + `.log`
- `benchmark_results/2026-06-24/quant_gemini-3.5-flash-nitro.json` + `.log`
- `benchmark_results/2026-06-24/projecte2e_gemini-3.5-flash-nitro_summary.md` + `.tsv`
- `system_tests/projecte2e/reports/20260624-181830/results.tsv` (partial `glm-5.2-nitro`)
- `system_tests/projecte2e/reports/20260624-190331/` (full `gemini-3.5-flash-nitro`)
- `docs/superpowers/plans/2026-06-24-review-and-run-benchmarks.md`
