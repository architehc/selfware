# Selfware Comprehensive Codebase Audit

**Date:** April 9, 2026  
**Version:** 0.3.0  
**Branch:** agent-20260405-145152  
**Auditors:** 6 parallel Claude agents (Architecture, Tests, Config, Code Quality, Docs, E2E)

---

## Executive Summary

Selfware is a ~330 KLOC Rust codebase organized into 37 public modules with a solid, working core agent loop (CLI -> Config -> Agent -> LLM -> Tool execution -> Output). The architecture is fundamentally sound with excellent patterns like deferred tool loading (60-80% context reduction) and multi-layer safety enforcement. However, the repo has accumulated **103.8 GB of test artifacts**, **62 markdown files** in root, **23 TOML configs** (13 unused), **3,450 unsafe unwrap() calls**, and several aspirational modules that are either stubs or broken. The supervision system can detect failures but cannot recover from them (restart/escalate are no-ops). The default config points to an expired ngrok tunnel. Browser automation is pure vaporware. Production readiness: **core agent loop is solid; peripheral subsystems need cleanup before any new feature work.**

---

## Module Health Matrix

| Module | LOC | Files | Status | Health | Notes |
|--------|-----|-------|--------|--------|-------|
| **agent/** | 34.9K | 32 | Active | **HEALTHY** | Core reasoning loop, well-maintained |
| **tools/** | 29K | 46 | Active | **HEALTHY** | 70+ tools, deferred loading works well |
| **ui/** | 28.8K | 44 | Active | **HEALTHY** | Clean TUI architecture (ratatui-based) |
| **cognitive/** | 26.7K | 27 | Active | **MIXED** | Core works; dream/memory_hierarchy/self_reference are stale |
| **orchestration/** | 18.8K | 34 | Active | **MIXED** | Swarm/coordinator work; workflow DSL experimental |
| **safety/** | 14.8K | 22 | Active | **HEALTHY** | Multi-layer validation, threat modeling |
| **testing/** | 13.6K | 13 | Active | **HEALTHY** | Contract testing, QA profiles |
| **observability/** | 10.8K | 8 | Active | **HEALTHY** | Good telemetry infrastructure |
| **analysis/** | 9.9K | 10 | Active | **HEALTHY** | BM25, HNSW vector search, code graph |
| **api/** | 5.4K | 5 | Active | **HEALTHY** | OpenAI-compatible, circuit breaker, retry |
| **config/** | 5.6K | 11 | Active | **HEALTHY** | Cascade loading, env var overrides |
| **session/** | 4.6K | 7 | Active | **HEALTHY** | Chat persistence, checkpointing |
| **cli/** | 3.7K | 4 | Active | **HEALTHY** | Comprehensive clap-based parsing |
| **supervision/** | 2.5K | 3 | Active | **VAPORWARE** | Health monitoring works; restart/escalate are stubs |
| **self_healing/** | 81K+27K | 2 | Feature-gated | **CONFLICT** | Both file AND directory exist (non-standard) |
| **evolution/** | 5.6K | 8 | Feature-gated | **STALE** | Well-designed, never called from anywhere |
| **consolidation/** | 1.6K | 7 | Feature-gated | **BROKEN** | Wrong feature gating breaks compilation |
| **bench_harness/** | 4.4K | 17 | Feature-gated | **BROKEN** | Module re-export error prevents compilation |
| **vlm_bench/** | 2.9K | 13 | Feature-gated | **ASPIRATIONAL** | Research/eval tool, not production |
| **browser/** | 65 lines | 1 | Active | **VAPORWARE** | All methods are no-ops (goto, click, screenshot) |
| **swl/** | ~10K | ~15 | Active | **EXPERIMENTAL** | Parser exists, not wired into CLI |
| **autoscaler.rs** | 6.6K | 1 | Dead | **DEAD CODE** | Not declared in lib.rs |
| **sampling.rs** | 7.6K | 1 | Dead | **DEAD CODE** | Not declared, superseded by config/model.rs |
| **cache.rs** | 6.3K | 1 | Dead | **DEAD CODE** | Not declared, superseded by session/cache.rs |

---

## Top 10 Critical Issues

### 1. SUPERVISION SYSTEM IS NON-FUNCTIONAL (Severity: CRITICAL)
- `restart_child()` at `src/supervision/mod.rs:210` only logs, never restarts
- `escalate()` at `src/supervision/mod.rs:247` only logs, no parent supervisor communication
- **Impact**: System can detect failures but cannot recover from them
- **Fix**: Implement actual component restart/escalation logic (8-12h effort)

### 2. 3,450 UNWRAP() CALLS — CRASH-ON-ERROR (Severity: CRITICAL)
- Top offenders: `agent/execution.rs` (154), `tools/file.rs` (136), `cognitive/memory_hierarchy/mod.rs` (96)
- **Safety module paradox**: Even `safety/sandbox.rs` has 15 unwraps, `safety/path_validator.rs` has 17
- **Impact**: Any runtime error crashes the agent with no recovery path
- **Fix**: Systematic replacement with `?` operator on critical paths (20-30h effort)

### 3. DEFAULT CONFIG POINTS TO DEAD ENDPOINT (Severity: CRITICAL)
- `selfware.toml` endpoint: `https://crazyshit.ngrok.io/v1` — ngrok tunnels expire
- **Impact**: Any user running `selfware` without explicit config will fail immediately
- **Fix**: Change to `http://localhost:8000/v1` or require explicit config (1h)

### 4. 103.8 GB OF TEST ARTIFACTS ON DISK (Severity: HIGH)
- `long_run_tests/`: 101 GB (56 subdirectories)
- 4-day test generated 74 GB in 5h48m before disk-full crash
- v5 system test generated 22 GB in 4 hours
- **Impact**: Disk exhaustion killed both major test runs; blocks future testing
- **Fix**: Archive old runs, implement disk quota monitoring, retention policy

### 5. BENCH-HARNESS COMPILATION ERROR (Severity: HIGH)
- `src/bench_harness/long_running/mod.rs` re-exports `TaskSetup` from `project.rs` but it's defined in `runner.rs`
- **Impact**: `--features bench-harness` cannot compile
- **Fix**: Move re-export or definition (15 min)

### 6. CONSOLIDATION FEATURE-GATE BROKEN (Severity: HIGH)
- `agent/checkpointing.rs` unconditionally imports `crate::consolidation::*` in tests
- **Impact**: Compilation fails with `--no-default-features`
- **Fix**: Wrap import in `#[cfg(feature = "consolidation")]` (15 min)

### 7. BROWSER MODULE IS PURE STUBS (Severity: HIGH)
- `src/browser/mod.rs` is 65 lines; `goto()`, `click()`, `screenshot()` return mock success
- Browser tools (`BrowserScreenshot`, `BrowserClick`, etc.) are registered and discoverable
- **Impact**: Agent discovers and "uses" browser tools that silently do nothing
- **Fix**: Either implement real Playwright/CDP integration or remove tools from registry

### 8. 5 NATIVE FUNCTION CALLING MISCONFIGURATIONS (Severity: HIGH)
- SGLang configs claim `--tool-call-parser qwen` but set `native_function_calling = false`
- Affected: `selfware-4090-qwen35-256k.toml`, `selfware-evolve-122b.toml`, 3 others
- **Impact**: Tool calling uses XML embedding instead of native format (less efficient)
- **Fix**: Set `native_function_calling = true` for SGLang configs (30 min)

### 9. SWL WORKFLOW SYSTEM NOT INTEGRATED (Severity: MEDIUM)
- Parser, runtime, and guardrails exist in `src/swl/`
- CLI `workflow run` command routes to YAML-based executor only, ignores SWL
- `#![allow(dead_code, unused_imports)]` at top of `src/swl/mod.rs`
- **Impact**: Entire SWL subsystem (~10K LOC) is unused
- **Fix**: Wire SWL parser -> lowering -> runtime into CLI handler (4-8h)

### 10. self_healing.rs STRUCTURAL CONFLICT (Severity: MEDIUM)
- Both `src/self_healing.rs` (81 KB) AND `src/self_healing/` directory exist
- Works via non-standard `mod executor;` import but violates Rust conventions
- **Fix**: Move to standard `self_healing/mod.rs` + `self_healing/executor.rs` pattern (1h)

---

## Top 10 Quick Wins

### 1. Delete 3 Dead Code Files (5 min, saves 20 KB)
- `src/autoscaler.rs` — not declared in lib.rs, never imported
- `src/sampling.rs` — not declared, superseded by config/model.rs
- `src/cache.rs` — not declared, superseded by session/cache.rs

### 2. Delete 13 Unused Config Files (5 min, reduces confusion)
- `selfware-27b-fixed.toml`, `selfware-4090-qwen35-256k.toml`, `selfware-4090-qwen35-9b-q8-vision.toml`
- `selfware-micro.toml`, `selfware-qwen35-optimized.toml`
- `selfware-evolve-cognitive.toml`, `selfware-evolve-fast.toml`, `selfware-evolve-tiny.toml`, `selfware-evolve-tools.toml`
- `selfware-auto-qwen3-5-27b.toml`, `selfware-auto-txn545-Qwen3-5-122B-A10B-NVFP4.toml`
- `selfware-text-primary-local.toml`, `selfware-vision-primary-remote.toml`

### 3. Fix .gitignore (10 min, prevents future tracking issues)
Add:
```
/.selfware/tool_results/
/hello
/hello_test
/coverage-llvm/
.evolution-log.jsonl
.pytest_cache/
.mypy_cache/
```

### 4. Delete Root Binary Artifacts (1 min, saves 7.6 MB)
- `hello` (3.8 MB) and `hello_test` (3.8 MB) — compiled test binaries

### 5. Fix Vision Config Token Mismatch (1 min)
- `selfware-vision-primary-remote.toml`: `max_tokens = 192` but `token_budget = 180000`
- Change to `max_tokens = 8192`

### 6. Fix bench-harness Module Re-export (15 min)
- Move `TaskSetup` re-export in `src/bench_harness/long_running/mod.rs` to import from `runner` instead of `project`

### 7. Fix Consolidation Feature Gate (15 min)
- Wrap `agent/checkpointing.rs` test imports in `#[cfg(feature = "consolidation")]`

### 8. Delete Obsolete Test Scripts (5 min)
- `run_8hour_system_test.sh`, `run_8hour_system_test_v2.sh`, `run_8hour_system_test_v3.sh`, `run_8hour_system_test_v4.sh`
- `run_8h_system_marathon.sh`, `run_template_rust_batch.sh`, `run_greenfield_e2e_batch.sh`
- `run_e2e_diverse.sh`, `e2e_sequential.sh`, `spawn_10_agents.sh`, `spawn_10_long.sh`
- `run_guided_scheduler_lab.sh`, `continuous_4day_monitor.sh`

### 9. Fix Default Config Endpoint (1 min)
- `selfware.toml`: Change endpoint from ngrok to `http://localhost:8000/v1`

### 10. Remove Coverage Report from Repo (1 min, saves 9.9 MB)
- `tarpaulin-report.html` — should be CI artifact, not committed

---

## Recommended Immediate Actions (Before Any New Feature Work)

### Phase 1: Stop the Bleeding (1-2 hours)

1. Fix `selfware.toml` endpoint to `http://localhost:8000/v1`
2. Delete 3 dead source files (`autoscaler.rs`, `sampling.rs`, `cache.rs`)
3. Fix `.gitignore` additions
4. Delete root binaries (`hello`, `hello_test`) and coverage report (`tarpaulin-report.html`)
5. Fix bench-harness compilation error
6. Fix consolidation feature-gate

### Phase 2: Config Cleanup (1 hour)

7. Delete 13 unused config files
8. Fix 5 native_function_calling misconfigurations
9. Fix vision config token mismatch
10. Document which 10 configs to keep and their purposes

### Phase 3: Test Artifact Cleanup (2-4 hours)

11. Archive `long_run_tests/` runs older than 7 days (saves ~80 GB)
12. Delete duplicate/failed test runs (saves ~2.1 GB)
13. Delete 13 obsolete test scripts
14. Implement disk quota monitoring for future test runs
15. Clean dirty submodules in `bench_results/swebench/repos/`

### Phase 4: Code Quality Sprint (1-2 days)

16. Replace unwrap() on critical agent execution path (`agent/execution.rs`: 154 calls)
17. Replace unwrap() in tools/file.rs (136 calls) 
18. Implement supervision restart_child() and escalate()
19. Resolve self_healing.rs structural conflict
20. Either implement browser automation or remove stub tools from registry

### Phase 5: Documentation Consolidation (2-4 hours)

21. Archive ~25 stale/redundant markdown files to `docs/archive/`
22. Move 47 shell scripts to `scripts/` directory
23. Organize 10 remaining TOML configs into `configs/` directory
24. Update ARCHITECTURE_SUMMARY.md with current module health ratings

---

## Files & Directories Safe to Delete

### Source Code (20 KB)
| File | Size | Reason |
|------|------|--------|
| `src/autoscaler.rs` | 6.6 KB | Not declared in lib.rs, never imported |
| `src/sampling.rs` | 7.6 KB | Not declared, superseded by config/model.rs |
| `src/cache.rs` | 6.3 KB | Not declared, superseded by session/cache.rs |

### Root Artifacts (17.5 MB)
| File | Size | Reason |
|------|------|--------|
| `hello` | 3.8 MB | Compiled test binary |
| `hello_test` | 3.8 MB | Compiled test binary |
| `tarpaulin-report.html` | 9.9 MB | Coverage report (regenerate in CI) |

### Config Files (13 files, ~50 KB)
| File | Reason |
|------|--------|
| `selfware-27b-fixed.toml` | Duplicate of 27b-concurrency16 |
| `selfware-4090-qwen35-256k.toml` | Hardware-specific, unmaintained |
| `selfware-4090-qwen35-9b-q8-vision.toml` | Hardware-specific, unmaintained |
| `selfware-micro.toml` | No active use |
| `selfware-qwen35-optimized.toml` | Experimental, incomplete |
| `selfware-evolve-cognitive.toml` | No code reference |
| `selfware-evolve-fast.toml` | No code reference |
| `selfware-evolve-tiny.toml` | No code reference |
| `selfware-evolve-tools.toml` | No code reference |
| `selfware-auto-qwen3-5-27b.toml` | Auto-generated, obsolete |
| `selfware-auto-txn545-Qwen3-5-122B-A10B-NVFP4.toml` | Auto-generated, obsolete |
| `selfware-text-primary-local.toml` | Research variant, unclear status |
| `selfware-vision-primary-remote.toml` | Broken token config (max_tokens=192) |

### Test Artifacts (~91 GB recoverable)
| Directory | Size | Reason |
|-----------|------|--------|
| `long_run_tests/results_20260401_*` (3 dirs) | 1.2 GB | Keep only latest |
| `long_run_tests/system_test_8hr_v2_*` | 278 MB | Superseded by v5 |
| `long_run_tests/system_test_8hr_v3_*` | 227 MB | Superseded by v5 |
| `long_run_tests/system_test_8hr_v4_*` (3 dirs) | 418 MB | Superseded by v5 |
| `swebench_eval/*` | 700 KB | Failed runs |
| `swebench_122b/*` | 50 KB | Incomplete |
| Old long_run_tests (>7 days) | ~88 GB | Archive to external storage |

### Test Scripts (13 files)
| Script | Reason |
|--------|--------|
| `run_8hour_system_test.sh` | Replaced by v5 |
| `run_8hour_system_test_v2.sh` | Replaced by v5 |
| `run_8hour_system_test_v3.sh` | Replaced by v5 |
| `run_8hour_system_test_v4.sh` | Replaced by v5 |
| `run_8h_system_marathon.sh` | Obsolete variant |
| `run_template_rust_batch.sh` | Obsolete |
| `run_greenfield_e2e_batch.sh` | Obsolete |
| `run_e2e_diverse.sh` | Obsolete |
| `e2e_sequential.sh` | Obsolete |
| `spawn_10_agents.sh` | Obsolete spawner |
| `spawn_10_long.sh` | Obsolete spawner |
| `run_guided_scheduler_lab.sh` | Obsolete |
| `continuous_4day_monitor.sh` | Superseded |

### Markdown Files (Archive ~25 to `docs/archive/`)
Redundant assessment docs, stale test reports, old fix status reports, and cleanup plans from March 2026 that no longer reflect current code state.

### Estimated Total Savings
| Category | Savings |
|----------|---------|
| Test artifacts (archive/delete) | ~91 GB |
| Root binaries + coverage | 17.5 MB |
| Config files | 50 KB |
| Dead source code | 20 KB |
| **Total** | **~91 GB** |

---

## Appendix A: Working Execution Flow

```
CLI (clap) --> Config::load() --> Agent::new() --> run_execution_loop()
                  |                    |                    |
                  v                    v                    v
          selfware.toml        ToolRegistry          Planning State
          env overrides        SafetyChecker              |
          API key chain        AgentMemory                v
                               CognitiveState       Executing State
                                                         |
                                                         v
                                               get_assistant_step_response()
                                                         |
                                                         v
                                               HTTP POST {endpoint}/v1/chat/completions
                                                  (retry on 429/5xx, circuit breaker)
                                                         |
                                                         v
                                               collect_tool_calls()
                                                         |
                                                         v
                                               execute_tool_batch()
                                                  validate schema
                                                  safety check + confirm
                                                  tool.execute()
                                                  store result (spill >50K tokens)
                                                         |
                                                         v
                                               check_completion_gate()
                                                         |
                                                    loop or done
```

## Appendix B: Config File Recommendations

**Keep (10 files):**
1. `selfware.toml` — Primary (FIX endpoint)
2. `selfware.example.toml` — Template
3. `selfware-hybrid.toml` — Multi-model reference
4. `selfware-eval.toml` — Benchmark reference
5. `selfware-extended-test.toml` — Long test reference
6. `selfware-stress-test.toml` — Stress/concurrency (add Docker note)
7. `selfware-longrun.toml` — 6-8h autonomous reference
8. `selfware-122b-concurrency64.toml` — Remote 122B high-throughput
9. `selfware-27b-concurrency16.toml` — Local 27B baseline
10. `selfware-evolve-122b.toml` — Evolution primary

## Appendix C: Feature Flag Status

| Feature | Default | Status | Notes |
|---------|---------|--------|-------|
| tui | ON | Working | TUI dashboard |
| workflows | ON | Working | Workflow execution (YAML only, SWL not wired) |
| resilience | ON | Working | Self-healing engine (structural conflict) |
| execution-modes | ON | Working | Auto-edit, dry-run modes |
| cache | ON | Working | Response caching |
| log-analysis | ON | Working | Log parsing |
| tokens | ON | Working | KV store |
| self-improvement | ON | Stale | Evolution engine (never called) |
| consolidation | OFF | Broken | Feature gate incorrect |
| vlm-bench | OFF | Aspirational | Research eval tool |
| bench-harness | OFF | Broken | Compilation error |

---

*Generated by 6 parallel Claude agents. Audit-only — no changes applied.*
