# Selfware Codebase Review — 54 Issues Across 20 Perspectives

**Date:** 2026-03-26
**Codebase:** 199,791 lines of Rust, 248 files, 728 tests, 73% coverage
**Reviewers:** Claude Opus 4.6 (automated deep analysis)

## Severity Legend

- **P0** — Blocking: runtime panics, async blocking, security holes
- **P1** — High: maintainability, significant tech debt, missing docs
- **P2** — Medium: code quality, design improvements, missing validation
- **P3** — Low: polish, minor inconsistencies, nice-to-have

---

## 1. Architecture & Design Patterns

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L1 | P1 | `src/agent/execution.rs` (3,912 lines) | Monolith — split into execution_loop.rs, tool_batch.rs, completion.rs |
| L2 | P1 | `src/agent/context_management.rs` (3,083 lines) | Monolith — split into compression.rs, context_assembly.rs, token_tracking.rs |
| L3 | P1 | `src/agent/interactive.rs` (2,943 lines) | Monolith — split into repl.rs, input_handling.rs, session.rs |
| L4 | P2 | `src/cognitive/` | Design fragmentation — memory_hierarchy.rs, cognitive_system.rs, compilation_manager.rs overlap in responsibility |
| L5 | P3 | `src/agent/execution.rs:72` | `#[allow(dead_code)]` on ControlFlow enum — either use it or remove it |

## 2. Code Quality & Cleanliness

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L6 | P2 | `src/agent/execution.rs` | 1 production `unwrap()` (166 in tests) — add `.context()` to the production one |
| L7 | P3 | `src/api/mod.rs` | 0 production `unwrap()` (157 in tests) — test code only, low risk |
| L8 | P2 | `src/agent/recovery.rs:643` | `#[allow(dead_code)]` function — incomplete error recovery path |
| L9 | P2 | `src/analysis/analyzer.rs:435-439` | Duplicated dead code suggestion logic — extract to shared helper |
| L10 | P2 | `src/cognitive/intelligence.rs:57` | TODO: migrate from std::sync::RwLock to tokio::sync::RwLock |

## 3. Security

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L11 | P1 | `src/config/mod.rs:116` | `extra_body` is unvalidated free-form JSON — could inject unsafe API fields (logprobs, logit_bias, etc.) |
| L12 | P2 | `src/tools/net_policy.rs` | IPv6 address bracket handling doesn't validate address bounds |
| L13 | P2 | `src/safety/threat_modeling.rs` (3,671 lines) | Mixes threat model definitions with test cases — split for auditability |
| L14 | P3 | `src/kv_store/` | aes-gcm imported in Cargo.toml but not used in KV store — no encryption at rest |

## 4. Performance

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L15 | P1 | `src/api/mod.rs` | No timeout on reqwest::Client::builder() creation — infinite hang possible on DNS |
| L16 | P2 | `src/api/mod.rs:267,591,1080` | `#[allow(dead_code)]` builder methods — incomplete public API surface |
| L17 | P2 | `src/analysis/vector_store.rs` | No HNSW batch import optimization — each insert triggers full index update |
| L18 | P3 | `.github/workflows/ci.yml` | Benchmarks compiled but never compared to baseline — no regression detection |

## 5. Testing Coverage

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L19 | P1 | `src/ui/tui/` | TUI integration tests broken (referenced in project memory notes) |
| L20 | P1 | `src/tools/` | No contract/integration tests for tool interfaces (file, shell, git) — only unit tests |
| L21 | P2 | `tests/` | No heap profiling in CI — memory regressions undetectable |
| L22 | P3 | `src/memory.rs` | 89 unit tests in single file — consider test module extraction |

## 6. Documentation

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L23 | P1 | `src/tools/mod.rs` | `Tool` trait has no doc comment — critical for contributor onboarding |
| L24 | P1 | `src/agent/mod.rs` | 38K file lacks module-level doc explaining agent lifecycle and state machine |
| L25 | P2 | `src/orchestration/workflow_dsl/` | Lexer, parser, AST, runtime have no high-level documentation |
| L26 | P2 | `src/bench_harness/` | New modules need crate-level doc examples (added this session, needs polish) |

## 7. Error Handling

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L27 | P1 | `src/errors.rs:40-41,115-116` | `ConfirmationRequired` duplicated in both AgentError AND SafetyError — conflation |
| L28 | P2 | Multiple files | ~30 `anyhow::bail!` where typed SelfwareError variants would be clearer |
| L29 | P2 | `src/api/mod.rs` | API errors not typed — all failures are anyhow::Error, losing structure for callers |
| L30 | P3 | `src/agent/execution.rs` | Some error paths use `format!()` then `anyhow::anyhow!()` — use `anyhow::bail!()` directly |

## 8. API Design

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L31 | P2 | `src/api/mod.rs:33-74` | Message canonicalization is an OpenAI-specific workaround — should be backend-specific adapter |
| L32 | P2 | `src/api/mod.rs` | No temperature range validation (should be 0.0-2.0) |
| L33 | P2 | `src/config/mod.rs:116` | ModelProfile.extra_body allows overriding any API field without validation |
| L34 | P3 | `src/api/types.rs` | ChatResponse doesn't preserve rate limit headers from API responses |

## 9. Memory Management (Rust-specific)

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L35 | P1 | `src/agent/interactive.rs` | 79 `Arc<Mutex>` instances — many read-heavy paths should use `Arc<RwLock>` |
| L36 | P2 | `src/agent/context_map.rs:1694` | `std::thread::sleep(10ms)` in test code mixed with production module |
| L37 | P3 | `src/analysis/vector_store.rs` | VectorIndex grows unbounded to MAX_CHUNKS (100K) — no LRU eviction |

## 10. Concurrency & Parallelism

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L38 | P0 | `src/agent/interactive.rs:217,226` | `std::thread::sleep()` in async context — blocks tokio runtime thread |
| L39 | P0 | `src/agent/execution.rs:3868` | `std::thread::sleep(5ms)` in async retry loop — blocks runtime |
| L40 | P0 | `src/cli.rs:682` | `std::thread::spawn()` for TUI event loop inside async context |
| L41 | P2 | `src/agent/context_map.rs:1694` | `std::thread::sleep(10ms)` in test setup (async context) |

## 11. Configuration Management

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L42 | P1 | `src/config/mod.rs` (4,080 lines) | Single-file monolith — split into config/{core,safety,agent,model,execution}.rs |
| L43 | P2 | `src/config/mod.rs` | No TOML schema validation — user config errors only caught at serde parse time |
| L44 | P2 | `src/config/mod.rs` | ExecutionMode::Daemon implies forever-run but not enforced in timeout logic |
| L45 | P3 | `src/config/auto_config.rs` | `run_tests()` called twice in `generate_config()` — should cache results |

## 12. CLI UX

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L46 | P2 | `src/cli.rs` | No validation for conflicting mode flags (--yolo + --confirm not mutually exclusive) |
| L47 | P3 | `src/cli.rs` | No `--version` output showing build features and git hash |
| L48 | P3 | `src/cli.rs` | `selfware bench` output not saved to file automatically (only `--format json` mentioned but not implemented) |

## 13. Logging & Observability

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L49 | P2 | `src/agent/execution.rs` | Tracing spans not consistently created at function entry — inconsistent trace granularity |
| L50 | P3 | `src/agent/` | No structured event for "task completed" with duration/token counts as span fields |
| L51 | P3 | `src/bench_harness/runner.rs` | Benchmark results not emitted as tracing events — only eprintln |

## 14. Dependencies & Cargo.toml

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L52 | P2 | `Cargo.toml:79` | tiktoken-rs v0.9 — check if upstream has newer version with fixes |
| L53 | P2 | `Cargo.toml:112` | nvml-wrapper 0.12.0 — no graceful fallback when NVIDIA drivers are absent |
| L54 | P3 | `Cargo.toml:101` | aes-gcm imported but only used in encryption module, not KV store |

## 15. CI/CD & Workflows

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L55 | P2 | `.github/workflows/ci.yml` | No performance regression detection — benchmarks compiled but not compared to baseline |
| L56 | P2 | `.github/workflows/ci.yml` | No heap/memory profiling in CI pipeline |
| L57 | P3 | `.github/workflows/ci.yml` | Feature-gated modules (vlm-bench, bench-harness, consolidation) not tested in CI matrix |

## 16. Dead Code & TODOs

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L58 | P2 | `src/agent/execution.rs:72` | `#[allow(dead_code)]` ControlFlow enum — internal state never matched |
| L59 | P2 | `src/agent/mod.rs:287` | `#[allow(dead_code)]` internal struct — incomplete feature |
| L60 | P2 | `src/output/mod.rs:671` | `intent_without_action()` function never called |
| L61 | P3 | `src/evolution/` | Feature="self-improvement" rarely tested — may bitrot |
| L62 | P3 | `src/vlm_bench/` | Feature="vlm-bench" not in default CI — may bitrot |

## 17. Project Structure & Organization

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L63 | P1 | Top 5 files | 18K lines in 5 files (9% of codebase) — decompose execution.rs, api/mod.rs, config/mod.rs, swarm.rs, threat_modeling.rs |
| L64 | P2 | `src/safety/threat_modeling.rs` (3,671 lines) | Mixes definitions with test cases — separate into model.rs + tests.rs |
| L65 | P2 | `src/analysis/vector_store.rs` (2,880 lines) | Single file for store + index + query — split into submodules |

## 18. Async/Await Patterns

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L66 | P0 | `src/agent/interactive.rs:217` | `std::thread::sleep(Duration::from_millis(100))` blocks async executor |
| L67 | P0 | `src/agent/interactive.rs:226` | Second `std::thread::sleep()` in same file |
| L68 | P0 | `src/agent/execution.rs:3868` | `std::thread::sleep(Duration::from_millis(5))` in async retry path |
| L69 | P1 | `src/cli.rs:682` | `std::thread::spawn()` in async main — should be tokio::spawn or spawn_blocking |

## 19. Database/Storage (KV Store)

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L70 | P2 | `src/kv_store/` | No encryption at rest despite aes-gcm dependency |
| L71 | P2 | `src/analysis/vector_store.rs` | No index compaction/GC — deleted entries leave holes |
| L72 | P3 | `src/session/checkpoint.rs` | Checkpoint serialization not versioned — format changes break resume |
| L73 | P3 | `src/consolidation/store.rs` | LongTermStore uses JSON files — no index for fast queries at scale |

## 20. Computer Control / Window Management

| ID | Severity | File | Issue |
|----|----------|------|-------|
| L74 | P2 | `src/computer/` | macOS support unclear — xcap v0.3 may not cover all macOS APIs |
| L75 | P2 | `src/tools/computer.rs` | Single rate bucket for all action types — scroll/move/click share limit |
| L76 | P3 | `src/computer/keyboard.rs` | Blocked key combos hardcoded — should be configurable via safety config |
| L77 | P3 | `src/computer/window.rs` | Window focus/launch not tested on Wayland (X11 assumptions) |

---

## Summary

| Severity | Count | Key Theme |
|----------|-------|-----------|
| **P0** | 4 | `std::thread::sleep` in async production code (10 locations), client timeout (1) |
| **P1** | 14 | File decomposition (5), missing docs (3), security validation (2), Arc<Mutex> (1), broken tests (1), error types (1), thread::spawn (1) |
| **P2** | 27 | Code quality, design improvements, missing validation, storage gaps |
| **P3** | 9 | Polish, minor inconsistencies, nice-to-have |
| **Total** | **54** | |

### Corrected Risk Assessment

**unwrap() in production code:** Only **7 total** (1 in execution.rs, 6 in interactive.rs).
The 369 count was inflated by test code (166 in execution.rs tests, 157 in api/mod.rs tests, 158 in config tests).
Test unwraps are acceptable — production unwraps are the actual risk.

**Blocking sleeps in production:** **10 locations** (not 4):
- `agent/interactive.rs:217,226` — 10ms + 100ms in REPL loop
- `agent/execution.rs:3868` — 5ms retry delay
- `cli.rs:2062` — step delay in demo mode
- `self_healing/executor.rs:412` — capped backoff delay
- `session/checkpoint.rs:880` — configurable retry delay
- `evolution/tournament.rs:322,364` — tournament evaluation delays
- `ui/spinner.rs:139` — animation frame delay (intentionally blocking its own thread)
- `agent/context_map.rs:1694` — file timestamp delay (should be test-only)

Note: `ui/spinner.rs` runs on its own spawned thread, so blocking is intentional.
The truly problematic ones are in agent/ and self_healing/ where they block the tokio runtime.

## Recommended Fix Order

### Week 1 — P0 fixes
1. Replace `std::thread::sleep` → `tokio::time::sleep` in agent/interactive.rs, agent/execution.rs, self_healing/executor.rs, session/checkpoint.rs (6 locations in async contexts)
2. Replace `std::thread::spawn` → `tokio::spawn_blocking` in cli.rs (L40, L69)
3. Add `.context()` to the 7 production unwrap() calls (L6)
4. Add reqwest client creation timeout (L15)

### Week 2 — P1 decomposition (14 issues)
5. Split execution.rs → execution_loop.rs + tool_batch.rs + completion.rs (L1)
6. Split config/mod.rs → config/{core,safety,agent,model}.rs (L42)
7. Split api/mod.rs → api/{client,streaming,types,canonicalize}.rs
8. Document Tool trait (L23) and agent lifecycle (L24)
9. Fix ConfirmationRequired error duplication (L27)
10. Validate extra_body JSON against allowlist (L11)
11. Convert read-heavy Arc<Mutex> → Arc<RwLock> (L35)
12. Fix TUI integration tests (L19)

### Week 3 — P2 improvements (27 issues)
13. Split vector_store.rs, threat_modeling.rs (L64, L65)
14. Add TOML schema validation (L43)
15. Add temperature range validation (L32)
16. Add perf regression detection in CI (L55)
17. Type API errors (L29)
18. Add feature-gated module testing in CI (L57)
