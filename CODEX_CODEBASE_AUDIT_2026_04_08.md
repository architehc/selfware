# CODEX Codebase Audit

Snapshot date: 2026-04-09
Repo: `selfware`
Branch: `agent-20260405-145152`

## Executive Summary

Selfware has one real product path: single-agent CLI -> config loader -> agent runtime -> API client -> tool execution. That path compiles, basic workflow commands run, and the checked-in remote endpoint is reachable. Everything around that core is much less healthy. The repo is carrying over 100 GB of test artifacts, the `--all-features` build is broken, several CLI surfaces are explicit placeholders, multi-agent/coordinator behavior is overstated, SWL guardrails are lowered to warnings instead of enforced behavior, and the config/doc surface has drifted away from the actual endpoint and runtime. This is not a repo that should take new feature work before cleanup.

## Audit Basis

- `cargo check` passed on the default path, but emitted dead-code warnings concentrated in orchestration.
- `cargo check --all-targets --all-features` failed in `bench_harness` and CLI long-run reporting paths.
- `cargo test --test swl_runtime_test` passed.
- `cargo test --lib orchestration::workflows::test_execution -- --nocapture` passed.
- `cargo run --bin selfware -- workflow validate workflows/test_execution.swl` passed.
- `cargo run --bin selfware -- workflow run workflows/test_execution_simple.yml --dry-run` passed.
- `cargo run --bin selfware -- workflow run workflows/visual_test.swl --dry-run` passed.
- `cargo run --bin selfware -- workflow run workflows/visual_test.swl` completed live with the remote endpoint.
- `curl https://crazyshit.ngrok.io/v1/models` on 2026-04-09 returned one served model:
  `/media/thread/trebuchet6/qwen35/models/Qwen3.5-122B-A10B-NVFP4-yarn-1010k`
  with `max_model_len = 1010000`.
- Workspace storage snapshot:
  - `long_run_tests/`: `101G`
  - `target/`: `14G`
  - `bench_results/`: `792M`
  - `gpu_max_test/`: `4.9M`
  - `.selfware/`: `3.0M`

## Module Health Matrix

| Area | Status | Health | Notes |
|---|---|---|---|
| `src/main.rs` | active | good | Thin entry point. Starts optional health endpoint and hands off to CLI. |
| `src/cli/` | active | mixed | Core commands work, but `batch`, `validate`, and `swebench` include explicit placeholder bail-outs. |
| `src/config/` | active | mixed | Loader is real and tested; defaults and checked-in configs are stale against the current endpoint. |
| `src/api/` | active | good | Real chat/chat-stream client with context budgeting and timeout handling. |
| `src/agent/` | active | mixed | Real single-agent loop with tool execution and safety checks; also contains brittle fallbacks and scattered unwrap risk. |
| `src/tools/` | active | mixed | Real tool registry and tool implementations, but safety is enforced in `Agent`, not centralized in `ToolRegistry`. |
| `src/safety/` | partial | mixed | `SafetyChecker` and permission gating are real; broader sandbox/autonomy/yolo story is mostly unproven or decorative. |
| `src/supervision/` | partial | weak | Health endpoint is wired; restart/escalation logic is stubby. |
| `src/swl/` | partial | weak | Parser/lowering/test coverage exists, but guardrails are not enforced and semantics are narrower than advertised. |
| `src/orchestration/workflows.rs` | active | mixed | Executor is real for log/LLM flows. CLI wiring does not clearly attach a tool handler for real tool workflows. |
| `src/orchestration/multiagent/` | partial | weak | Parallel fan-out exists, but agents do not actually get tools in the chat path. |
| `src/orchestration/coordinator.rs` | broken/aspirational | poor | Restriction model is not real, worker execution is simulated, and code shows up as dead. |
| `src/orchestration/scratchpad.rs` | stale | poor | Exported surface with dead-code warnings and no convincing runtime role. |
| `src/batch/` | aspirational | poor | Batch executor is explicitly stubbed and the CLI bails rather than pretend success. |
| `src/validation/` | aspirational | poor | CLI says scoring/reporting are placeholder-only; module advertises fabricated scores. |
| `src/cognitive/` | active + aspirational mix | mixed | Core cognitive state/memory pieces are used by the agent. Advanced self-improvement/RSI surfaces are feature-gated and lightly proven. |
| `src/consolidation/` | partial | weak | Real code exists and checkpointing references it behind the feature flag, but it is not a central runtime path. |
| `src/evolution/` | aspirational | weak | CLI entry exists, but the overall surface is feature-gated and not convincingly validated end-to-end. |
| `src/self_healing.rs` + `src/self_healing/` | active | fair | Not a module conflict. This is an intentional file-module plus submodule layout. Used by agent checkpointing/error recovery. |
| `src/browser/` | broken/stub | poor | Placeholder module; no convincing production wiring in the scoped review. |
| `src/computer/` | partial | fair | Real Linux/WSL-heavy control code. Uneven platform coverage and some missing macOS functionality. |
| `src/swebench/` | broken/placeholder | poor | CLI says SWE-bench eval is not implemented; module contains demo-task behavior and placeholder success. |
| `src/bench_harness/` | broken | poor | All-feature build currently fails here. |

## Architecture Map

The credible runtime graph is:

`src/main.rs`
-> `src/cli/mod.rs`
-> `src/config/loader.rs`
-> `src/agent/mod.rs`
-> `src/api/client.rs`
-> `src/tools/mod.rs`

Supporting paths wired into that core:

- `src/safety/` through agent-side tool validation and permission checks
- `src/cognitive/` through agent state, memory, learning, checkpointing
- `src/self_healing.rs` through checkpoint/error recovery
- `src/supervision/health.rs` only for the optional health endpoint

Parallel or competing execution stories:

- YAML workflows run through `WorkflowExecutor`.
- SWL is parsed and lowered into `WorkflowExecutor`.
- `src/swl/runtime/mod.rs` is a second runtime with narrower, heuristic behavior.
- Multi-agent orchestration is a separate fan-out path that does not expose tools in the main chat flow.
- Coordinator mode is exported and documented more heavily than it is actually wired.

Structural verdict:

- `src/cli/mod.rs` is the control-plane bottleneck. It is the real architectural hub and directly dispatches workflows, evolution, validation, benchmarks, long tests, and UI concerns.
- There is no hard Rust circular dependency problem visible in the default build.
- The actual architectural failure is centralization plus feature sprawl: optional systems branch directly off `cli`, so disconnected code can accumulate without breaking the default binary.
- `Cargo.toml` default features are already broad and include experimental cross-cutting systems. The feature matrix is therefore larger than the maintained runtime truth.

## Top 10 Critical Issues

| Rank | Severity | Issue | Evidence | Impact |
|---|---|---|---|---|
| 1 | critical | `--all-features` build is broken | `src/bench_harness/long_running/mod.rs:59`, `src/bench_harness/long_running/project.rs:208`, `src/cli/mod.rs:2167` | Feature matrix is not shippable or CI-trustworthy. |
| 2 | critical | Several CLI entrypoints are explicit placeholders | `src/cli/mod.rs:1198`, `src/cli/mod.rs:1222`, `src/cli/mod.rs:1825` | Users are offered commands that intentionally do not work. |
| 3 | critical | Repo is drowning in test artifacts | `long_run_tests/` alone is `101G`; `system_test_8hr_v5_20260405_082729` ends with `No space left on device` | Storage pressure corrupted or invalidated later test runs. |
| 4 | high | Multi-agent is not real multi-agent tool use | `src/orchestration/multiagent/chat.rs:214`, `src/orchestration/multiagent/chat.rs:269` | Marketed coordination is mostly parallel prompt fan-out. |
| 5 | high | Coordinator mode is mostly simulated | `src/orchestration/coordinator.rs:230`, `src/orchestration/coordinator.rs:569`, `src/orchestration/coordinator.rs:777` | Exported architecture does not match runtime truth. |
| 6 | high | SWL guardrails are not enforced | `src/swl/lowering.rs:524`, `src/swl/guardrails/engine.rs:421` | Workflow safety semantics are weaker than the language implies. |
| 7 | high | Default config has drifted from the live endpoint | `selfware.toml` still uses `txn545/Qwen3.5-122B-A10B-NVFP4` and `262144`, while the endpoint on 2026-04-09 exposed one model at `1010000` max context | Default behavior is under-documented and potentially misconfigured. |
| 8 | high | Nested SWE-bench repos are in an invalid hygiene state | `git submodule status` fails, but `git status` still reports dirty nested repos under `bench_results/swebench/repos/*` | Dirty worktrees recur and ownership of embedded repos is unclear. |
| 9 | high | Docs overclaim maturity and are drifting from code | `README.md`, `ARCHITECTURE_SUMMARY.md`, `docs/configuration.md`, `CHANGELOG.md` | Engineers cannot trust docs as a source of truth. |
| 10 | medium | Safety enforcement is runtime-path-specific, not systemic | `src/tools/mod.rs:565`, `src/orchestration/parallel/executor.rs:131`, `src/agent/tool_execution.rs:199` | Alternative execution paths can bypass the main safety contract. |

## Test Infrastructure And Results

### Summary Table

| Run family | Verdict | Notes |
|---|---|---|
| `greenfield_batch_20260402_220920` | pass | All 3 projects completed. |
| `diverse_e2e_20260402_224717` | pass | `calculator` and `roman` green. |
| `focused_followup_20260404_154235` | fail | `viz_ascii_table` stayed `needs_work`. |
| `marathon_20260402_230101` | partial | 31 rounds recorded; template rounds still needed work. |
| `marathon_20260404_232000` | partial | Some recovery, but `viz_ascii_table` kept regressing. |
| 8-hour system tests `v1..v5` | partial -> best at `v4` -> invalid at `v5` | Progress improved through `v4`; `v5` collapsed under disk pressure. |
| 4-day harness | incomplete/aborted | No final report or clean completion marker. |
| `swebench_eval/20260325_105727` | unknown | Runner said tasks completed, but there was no final report artifact. |
| `swebench_eval/20260325_121127` | fail | Explicit `FAILED - No tasks completed`. |
| `swebench_122b/*` | hang/aborted | Both runs stop at startup. |
| `gpu_max_test/*` | fail/no-op | Final reports show `0` tasks and `0h 0m`. |
| `bench_results/continuous` | pass with weakness | Throughput and multilingual passed; browser only `2/4`. |
| `bench_results/swebench/two_phase` | reporting conflict | One report says `292/300`; another resolves only `6/293`. |

### 8-Hour System Test Progression

- `v1`: baseline completed, but weak. Final summary reported `8 GREEN, 3 PARTIAL / 25`.
- `v2`: attempted to fix empty-project completion logic, but regressed to `6 GREEN, 4 PARTIAL / 26`.
- `v3`: real improvement. Reported `11 GREEN, 2 PARTIAL / 26`, but the harness itself logged parsing and grep errors.
- `v4`: best credible version. Reported `18 GREEN, 5 PARTIAL / 42`.
- `v4` rerun on 2026-04-05: exposed recurring failure modes instead of finishing cleanly. Status showed `23 GREEN, 8 PARTIAL, 6 COMPILES, 13 WROTE / 50`, then stalled on `verification_missing`, `edit_failure_loop`, and `timeout`.
- `v5`: cycle 1 looked better for multi-language tasks, then the run collapsed into invalid result rows and ended with `No space left on device`. Aggregate counts are not trustworthy.

### 4-Day Harness Verdict

- No evidence of a completed 96-hour run.
- `long_run_tests/4day_results_20260405_094416/orchestrator.log` shows several hours of work on 2026-04-05, not a four-day completion.
- `long_run_tests/4day_results_20260405_094154` is effectively empty.
- Verdict: archive the 4-day harness until it can produce one complete, reportable run.

### Current Vs Obsolete Scripts

Keep as current evidence-bearing harnesses:

- `long_run_tests/run_8hour_system_test_v4.sh`
- `long_run_tests/monitor_8hour_test.sh`
- `long_run_tests/check_status.sh`
- `long_run_tests/run_daily_8hour.sh`
- `long_run_tests/run_greenfield_e2e_batch.sh`
- `long_run_tests/run_template_rust_batch.sh`
- `long_run_tests/run_e2e_diverse.sh`
- `long_run_tests/run_8h_system_marathon.sh`

Archive as superseded or experimental:

- `long_run_tests/run_8hour_system_test.sh`
- `long_run_tests/run_8hour_system_test_v2.sh`
- `long_run_tests/run_8hour_system_test_v3.sh`
- `long_run_tests/run_8hour_system_test_v5.sh`
- `long_run_tests/run_8hour_system_test_v6.sh`
- `long_run_tests/monitor_v5_test.sh`
- `long_run_tests/monitor_v6_test.sh`
- the entire 4-day harness stack under `long_run_tests/` until it produces a clean full run

Delete or move out of git:

- all result trees under `long_run_tests/system_test_8hr_*`
- all result trees under `long_run_tests/marathon_*`
- all result trees under `long_run_tests/greenfield_batch_*`
- all result trees under `long_run_tests/diverse_e2e_*`
- all result trees under `long_run_tests/longrun_*`
- all result trees under `long_run_tests/results_*`
- all result trees under `long_run_tests/4day_results_*`
- `bench_results/swebench/repos/`
- `bench_results/swebench/workdirs/`
- `gpu_max_test/`
- generated report/work trees under `system_tests/projecte2e/reports` and `system_tests/projecte2e/work`

## Configuration And TOML Sprawl

### What Is Real

- The loader path is real and sane: explicit `--config`, then `SELFWARE_CONFIG`, then `selfware.toml`.
- The checked-in default config is `selfware.toml`.
- Test scripts and docs overwhelmingly assume `selfware.toml`, not the many variant files.

### What Is Stale Or Conflicting

- Remote configs still point at `txn545/Qwen3.5-122B-A10B-NVFP4` with `context_length = 262144`.
- On 2026-04-09, the remote endpoint exposed one model at
  `/media/thread/trebuchet6/qwen35/models/Qwen3.5-122B-A10B-NVFP4-yarn-1010k`
  with `max_model_len = 1010000`.
- Local configs disagree wildly on context length: `50000`, `128000`, `256000`, `262144`, and `1010000`.
- `native_function_calling` flips between `true` and `false` without a trustworthy compatibility matrix.
- `hybrid`, `vision`, and `evolve` configs claim runtime modes that are only partially validated or clearly aspirational.
- Several checked-in configs rely on keys that the runtime does not actually consume:
  - `[parallel]`
  - ad hoc `[concurrency]` keys such as `max_parallel_requests`, `max_retries`, `timeout_secs`
  - `agent.max_parallel_tools`
  - many `population_size` / `generations` style evolution knobs
- Several configs omit `context_length`, which silently falls back to the code default `131072` instead of the value their comments imply.

### Recommended Canonical Config Set

Keep and maintain:

- `selfware.toml`
  - Make this the single default profile.
  - Update model identifier and context length to match the actual endpoint you want as default.
- `selfware.example.toml`
  - Keep as a generic user-editable template.
- `selfware-text-primary-local.toml`
  - Keep only if local text-first operation is a supported tier.
- `selfware-vision-primary-remote.toml`
  - Keep only if you explicitly document that vision/computer/browser flows are partial and platform-dependent.
- `selfware-evolve-122b.toml`
  - Keep only if `selfware evolve` remains a supported workflow.

Archive for now:

- `selfware-hybrid.toml`
- all `selfware-evolve-*.toml`
- `selfware-eval.toml`
- `selfware-longrun.toml`
- `selfware-extended-test.toml`
- `selfware-122b-concurrency64.toml`
- `selfware-27b-concurrency16.toml`
- `selfware-27b-fixed.toml`
- `selfware-auto-qwen3-5-27b.toml`
- `selfware-auto-txn545-Qwen3-5-122B-A10B-NVFP4.toml`
- `selfware-qwen35-optimized.toml`
- `selfware-stress-test.toml`
- `selfware-micro.toml`
- `selfware-4090-qwen35-256k.toml`
- `selfware-4090-qwen35-9b-q8-vision.toml`

Reason: these files are mostly benchmark profiles, hardware-specific experiments, or configs for features that the repo does not currently prove end-to-end.

Specific note: `selfware-hybrid.toml`, `selfware-122b-concurrency64.toml`, and `selfware-27b-concurrency16.toml` are especially misleading because they advertise parallel/concurrency control through config sections the runtime does not honor.

## Code Quality And Rust Health

### Highest-Priority Quality Problems

- Placeholder surfaces are shipping as first-class CLI commands.
- Coordinator and scratchpad code are exported but mostly dead or simulated.
- Safety is real only on the main agent tool path; alternative orchestration paths are weaker.
- Multi-agent chat instantiates tools, then does not use them.
- SWL guardrails are warnings, not runtime policy.
- There are still panic-prone unwraps in runtime code paths.
- Progress recovery can inject synthetic scaffolding into `src/lib.rs`, which is a brittle failure mode.

### Lint And Format Status

- `cargo check` passed but emitted 27 warnings, mostly dead code in orchestration.
- `cargo clippy --lib --bins -- -W clippy::unwrap_used -W clippy::expect_used -W clippy::todo` passed but emitted 185 warnings.
- `clippy.toml` is minimal and permissive.
- `rustfmt.toml` is minimal and does not carry repo-level quality policy.

Verdict: the repo is not failing because Rust is unmanaged; it is failing because too much code is being exported before it has one proven runtime path.

## Documentation And Repo Hygiene

- Repo root contains `62` markdown files. Most are one-off reports, plans, and generated reviews, not durable product docs.
- `README.md` is broadly aligned on branding and command surface, but overstates maturity.
- `ARCHITECTURE_SUMMARY.md` is stale and should not be treated as authoritative.
- `docs/configuration.md` is stale on model defaults.
- `docs/architecture.md` is the best of the sampled docs, but still cleaner than the runtime reality.
- `.gitignore` does not reflect actual artifact generation. It ignores `bench_results/` and `target/`, but not `long_run_tests/`, `.selfware/tool_results/`, `coverage-llvm/`, or the various generated test output directories.
- The repo has many timestamped `agent-*` branches with no visible pruning rule.
- The nested SWE-bench repos are neither clean submodules nor clean external clones.

### Cleanup Plan

- Keep root docs small: `README.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md`, `PRIVACY.md`.
- Move review/report debris into `docs/archive/` or delete it if superseded.
- Rewrite `README.md` around what works today.
- Rewrite `docs/configuration.md` directly from `src/config/mod.rs` and current endpoint behavior.
- Add ignore rules for `/.selfware/tool_results/`, `/long_run_tests/`, `/coverage-llvm/`, `/test_output_*/`, `/test_real_*/`, `/live_test_*/`, and `**/target/`.
- Decide whether SWE-bench repos are real submodules, external clones, or disposable workdirs. Stop keeping them in a half-state.
- Define and enforce a branch-retention rule for `agent-*`.

## Working Vs Broken Execution Paths

### Working Or Mostly Working

- single-agent CLI prompt/tool path
- config loading and override precedence
- basic YAML workflow execution for simple log/LLM flows
- SWL parse/validate/lower flows
- health endpoint startup
- core Linux/WSL computer-control primitives

### Partially Working

- SWL execution, but only for the narrowed lowered subset
- workflow execution for anything that needs real tool handlers
- safety outside the main agent path
- supervision beyond health reporting
- consolidation and self-healing
- evolution entrypoints

### Broken, Placeholder, Or Misleading

- CLI `batch`
- CLI `validate` as a real scoring pipeline
- CLI `swebench`
- coordinator mode as a real orchestrator
- multi-agent as real tool-using collaboration
- browser module under `src/browser/`
- SWE-bench runtime claims versus actual implementation
- 4-day long-run claims

## Top 10 Quick Wins

1. Delete or archive all result payloads under `long_run_tests/`.
2. Remove or hide placeholder CLI commands until they are real.
3. Make `cargo check --all-targets --all-features` green, or stop advertising that surface.
4. Update `selfware.toml` to the real endpoint model and context length.
5. Add missing `.gitignore` rules for test artifacts and tool result caches.
6. Pick one workflow runtime and document it as authoritative.
7. Collapse or quarantine dead orchestration surfaces (`coordinator`, `scratchpad`).
8. Rewrite `README.md` and `docs/configuration.md` from current code, not old reports.
9. Resolve nested SWE-bench repos into one clean ownership model.
10. Add a CI gate for the default runtime path plus one honest extended path, instead of pretending all exported modes are equally real.

## Recommended Immediate Actions Before New Feature Work

1. Freeze new features.
2. Purge the workspace of generated test and benchmark payloads.
3. Repair or remove broken exported surfaces:
   `--all-features`, `batch`, `validate`, `swebench`, `coordinator`.
4. Decide the product truth:
   single-agent first, or real multi-agent/coordinator. Stop carrying both stories simultaneously.
5. Update config and docs to the actual endpoint as of 2026-04-09.
6. Re-run one clean 8-hour system test after cleanup and disk recovery.
7. Add a small CI matrix that proves:
   default build, default tests, workflow tests, and one live-config smoke path.
8. Archive or delete stale audit/report markdown from repo root.

## Files And Directories Safe To Delete

These are safe to delete from the working tree now. Savings are approximate workspace savings, not git-history savings.

| Path | Approx. savings | Notes |
|---|---|---|
| `long_run_tests/4day_results_20260405_094416` | `74G` | Largest single artifact tree; no completed 4-day result. |
| `long_run_tests/system_test_8hr_v5_20260405_082729` | `22G` | Invalidated by disk exhaustion. |
| remaining `long_run_tests/*` result trees | `~5G` | Old system tests, marathon runs, batch runs, follow-ups. |
| `target/debug` | `15G` | Build artifact, not source. |
| `target/release` | `1.2G` | Build artifact, not source. |
| `bench_results/swebench/` | `~786M` | Includes dirty nested repos and workdirs. |
| `gpu_max_test/` | `4.9M` | No-op runs; zero-task reports. |
| `.selfware/tool_results/` | small | Generated cache, currently unignored. |

Rough total immediate workspace recovery: about `118G`.

## Bottom Line

This repo is not dead, but it is over-exported and under-pruned. The single-agent core is real. The surrounding claims about multi-agent orchestration, coordinator behavior, SWE-bench evaluation, long-horizon testing, and configuration breadth are materially ahead of the code. The next correct move is consolidation, not expansion.
