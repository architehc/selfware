# Consolidated Report: Broken Implementations in Selfware Codebase

**Generated:** 2026-03-12  
**Source:** Analysis from 10 specialized agents  
**Total Issues:** 73

---

## Executive Summary

This report consolidates findings from 10 specialized agents analyzing the Selfware codebase. Issues are categorized by priority:

| Priority | Count | Description |
|----------|-------|-------------|
| **CRITICAL (P0)** | 12 | Security vulnerabilities, blocking operations in async contexts, data loss risks |
| **HIGH (P1)** | 18 | Performance bottlenecks, missing core functionality, architectural flaws |
| **MEDIUM (P2)** | 32 | Stub implementations, incomplete features, code quality issues |
| **LOW (P3)** | 11 | WIP modules, minor TODOs, cosmetic issues |

---

## Detailed Issue Table

### CRITICAL (P0) - Immediate Action Required

| Priority | Module | File | Lines | Issue | Fix Required |
|----------|--------|------|-------|-------|--------------|
| P0 | safety | `src/safety/checker.rs` | 272-277 | Unknown tools ALLOWED by default - major security gap where unregistered tools pass through safety checks without blocking | Add explicit `anyhow::bail!()` for unknown tools or require registration |
| P0 | safety | `src/safety/path_validator.rs` | 180-198 | Path traversal check only checks for "..", allowing absolute paths to bypass validation entirely | Validate all path types including absolute paths; canonicalize before checking |
| P0 | safety | `src/safety/path_validator.rs` | 225-229 | Empty `allowed_paths` configuration allows ALL paths through validation | Treat empty allowed_paths as deny-all or require explicit configuration |
| P0 | safety | `src/safety/checker.rs` | 290-418 | Shell command bypass vectors: command substitution `$(...)`, backticks, variables can hide dangerous commands | Block all command substitution patterns before processing; sanitize variables |
| P0 | agent | `src/agent/checkpointing.rs` | 316-330 | Uses `std::thread::spawn()` instead of `tokio::task::spawn_blocking()` for file I/O in async context | Replace with `tokio::task::spawn_blocking()` for proper async runtime compatibility |
| P0 | agent | `src/agent/session_log.rs` | 86-98 | Uses `std::fs::create_dir_all()` blocking sync call | Replace with `tokio::fs::create_dir_all()` |
| P0 | agent | `src/agent/session_log.rs` | 139-161 | `read_recent()` is entirely blocking file read without async | Convert to async function using `tokio::fs::read_to_string()` |
| P0 | agent | `src/agent/execution.rs` | 1107-1108 | `read_line_pausing_esc()` calls `std::io::stdin().read_line()` blocking in async context | Use async input reading or dedicated blocking thread |
| P0 | agent | `src/agent/interactive.rs` | 454-518 | `editor.read_line()` blocking in async loop - will block entire runtime | Move to dedicated blocking thread or use async readline library |
| P0 | session | `src/session/cache.rs` | 541+ | `LlmCache` marked `#[allow(dead_code)]` - not integrated, no disk persistence | Integrate cache into main flow; add disk persistence with TTL |
| P0 | session | `src/session/local_first.rs` | 39-999 | Local-first implementation exists but is not integrated into main flow | Wire into session management; add pending ops persistence |
| P0 | observability | `src/session/chat_store.rs` | N/A | Encryption is optional, falls back to plaintext silently | Make encryption mandatory or require explicit opt-out with warning |

### HIGH (P1) - Fix Within Sprint

| Priority | Module | File | Lines | Issue | Fix Required |
|----------|--------|------|-------|-------|--------------|
| P1 | safety | `src/safety/yolo.rs` | 48-56 | Easily bypassed forbidden operations using simple variations | Implement fuzzy matching, tokenization, and normalization |
| P1 | safety | `src/safety/autonomy.rs` | 384-401 | Simple substring matching for paths - bypassable with URL encoding, Unicode | Use proper path canonicalization and normalization |
| P1 | analysis | `src/analysis/vector_store.rs` | 745-752 | Brute-force O(N) search algorithm - no HNSW indexing | Implement HNSW or IVF index for sub-linear search |
| P1 | analysis | `src/analysis/vector_store.rs` | 800-852 | Naive search iterates ALL embeddings - unusable at scale | Add approximate nearest neighbor (ANN) search |
| P1 | analysis | `src/analysis/vector_store.rs` | 59-60 | Hardcoded 100k limit due to brute force algorithm | Remove limit after implementing proper indexing |
| P1 | analysis | `src/analysis/vector_store.rs` | 856 | No SIMD dot product implementation - 4-8x slower | Add AVX2/NEON SIMD dot product for x86_64/aarch64 |
| P1 | analysis | `src/analysis/vector_store.rs` | 1399-1426 | `rebuild_index()` re-embeds all chunks unnecessarily | Add incremental index updates or use mutable index structure |
| P1 | cognitive | `src/cognitive/memory_hierarchy.rs` | 913-998 | Uses synchronous BM25 instead of semantic embeddings | Replace with dense vector embeddings for semantic search |
| P1 | orchestration | `src/orchestration/swarm.rs` | 670 | `std::sync::RwLock` used in async context | Replace with `tokio::sync::RwLock` |
| P1 | orchestration | `src/orchestration/parallel.rs` | 958-999 | `StdRwLock` mixed with async code | Replace with `tokio::sync::RwLock` |
| P1 | orchestration | `src/orchestration/swarm.rs` | 1004-1043 | Task assignment without execution loop - tasks never actually run | Implement proper task execution loop with worker pool |
| P1 | resource | `src/resource/gpu.rs` | 182-185 | GPU metrics commented out - no monitoring | Uncomment and fix metrics integration |
| P1 | resource | `src/resource/memory.rs` | 119-121 | Memory metrics commented out - no monitoring | Uncomment and fix metrics integration |
| P1 | resource | `src/resource/mod.rs` | 145-148 | Resource metrics commented out - no monitoring | Uncomment and fix metrics integration |
| P1 | observability | `src/observability/analytics.rs` | All | All in-memory only, no persistence across restarts | Add SQLite/Redis persistence layer |
| P1 | observability | `src/observability/carbon_tracker.rs` | All | Emission records in memory only | Add persistent storage for carbon audit trail |
| P1 | observability | `src/observability/dashboard.rs` | All | Session stats not persisted | Persist to disk for historical analysis |
| P1 | session | `src/session/encryption.rs` | All | No key rotation mechanism | Implement key rotation with re-encryption capability |

### MEDIUM (P2) - Fix Within Release Cycle

| Priority | Module | File | Lines | Issue | Fix Required |
|----------|--------|------|-------|-------|--------------|
| P2 | tools | `src/tools/knowledge.rs` | 744-754 | Weak path validation - blocks absolute paths entirely instead of validating | Implement proper path validation with whitelist/blacklist |
| P2 | tools | `src/tools/vision.rs` | 301-319 | No path traversal validation when reading image files | Add `validate_path()` call before file access |
| P2 | tools | `src/tools/screen_capture.rs` | 122-124 | Direct file write without path validation | Add path validation before write operation |
| P2 | tools | `src/tools/container.rs` | 714-720 | Environment variables not validated in `container_exec` | Add env var validation for injection attacks |
| P2 | tools | `src/tools/pty_shell.rs` | 85-95 | `$SHELL` env var used without validation | Validate shell path against allowlist |
| P2 | cognitive | `src/cognitive/learning.rs` | 289-314 | `generate_overall_explanation()` is minimal string concatenation | Implement LLM-based explanation generation |
| P2 | cognitive | `src/cognitive/learning.rs` | 316-330 | `describe_concept()` hardcoded with only 10 concepts | Expand concept database or use LLM for dynamic descriptions |
| P2 | cognitive | `src/cognitive/learning.rs` | 333-339 | `suggest_related_topics()` returns static hardcoded suggestions | Implement dynamic topic suggestion based on code analysis |
| P2 | cognitive | `src/cognitive/self_reference.rs` | 373-405 | `infer_key_components()` returns hardcoded lists | Implement AST-based component inference |
| P2 | cognitive | `src/cognitive/self_reference.rs` | 407-418 | `infer_dependencies()` hardcoded based on path matching | Use actual import analysis via tree-sitter |
| P2 | cognitive | `src/cognitive/self_reference.rs` | 420-434 | `infer_dependents()` hardcoded based on path matching | Use actual dependency graph from code analysis |
| P2 | cognitive | `src/cognitive/self_reference.rs` | 24 | `code_cache` HashMap has no eviction policy - unbounded growth | Add LRU eviction with size limit |
| P2 | cognitive | `src/cognitive/intelligence.rs` | 57-60 | TODO about migrating to `tokio::sync::RwLock` | Replace `std::sync::RwLock` with `tokio::sync::RwLock` |
| P2 | cognitive | `src/cognitive/self_improvement.rs` | 16-19 | TODO about `tokio::sync::RwLock` | Replace `std::sync::RwLock` with `tokio::sync::RwLock` |
| P2 | cognitive | `src/cognitive/learning.rs` | 341+ | `CodeExplainer.history` unbounded growth | Add max history limit with LRU eviction |
| P2 | cognitive | `src/cognitive/learning.rs` | N/A | `patterns` Vec unbounded growth | Add limit and deduplication |
| P2 | cognitive | `src/cognitive/learning.rs` | N/A | `access_count` is u32 (overflow risk) | Use u64 or saturating arithmetic |
| P2 | resource | `src/resource/disk.rs` | 261-265 | `cleanup_orphaned_files()` returns `Ok(0)` stub | Implement actual orphaned file detection and cleanup |
| P2 | resource | `src/resource/disk.rs` | 281-284 | `get_models_size()` returns hardcoded 10GB | Implement actual directory size calculation |
| P2 | resource | `src/resource/gpu.rs` | 191-200 | `throttle_compute()` only logs, doesn't throttle | Integrate with LLM engine to actually throttle |
| P2 | resource | `src/resource/gpu.rs` | 203-206 | `reduce_batch_size()` only logs | Actually communicate with inference engine |
| P2 | resource | `src/resource/memory.rs` | 48-76 | Most action handlers are empty stubs | Implement actual memory pressure responses |
| P2 | resource | `src/resource/mod.rs` | 263-266 | `ResourceReservation::release()` only logs | Actually release resources |
| P2 | supervision | `src/supervision/mod.rs` | 207-211 | `restart_child()` only logs | Implement actual component restart |
| P2 | supervision | `src/supervision/mod.rs` | 246-250 | `escalate()` only logs | Implement parent supervisor notification |
| P2 | computer | `src/computer/keyboard.rs` | 32-106 | All methods only log and sleep - no OS integration | Implement platform-specific keyboard automation (AppleScript, xdotool) |
| P2 | computer | `src/computer/mouse.rs` | 54-78 | macOS placeholder only, no actual implementation | Implement CoreGraphics mouse control |
| P2 | computer | `src/computer/mouse.rs` | 82-139 | `click`, `scroll`, `drag` are logging-only stubs | Implement actual mouse event injection |
| P2 | devops | `src/devops/container.rs` | 732-817 | `run()` builds command but never executes | Actually spawn container runtime process |
| P2 | devops | `src/devops/container.rs` | 817-845 | `stop/start/remove` simulate state only | Execute actual container runtime commands |

### LOW (P3) - Backlog / Cleanup

| Priority | Module | File | Lines | Issue | Fix Required |
|----------|--------|------|-------|-------|--------------|
| P3 | tools | `src/tools/lsp_tools.rs` | 1 | `#![allow(dead_code, unused_imports, unused_variables)]` - entire module is WIP | Complete LSP tool implementation or remove |
| P3 | tools | `src/tools/swarm_tool.rs` | 1 | `#![allow(dead_code)]` - WIP module | Complete swarm tool or remove |
| P3 | tools | `src/tools/page_controller.rs` | 1 | `#![allow(dead_code)]` - WIP module | Complete Playwright bridge or remove |
| P3 | orchestration | `src/orchestration/visual_loop.rs` | 1-267 | Entire file is stub - data structures only, no execution | Implement visual feedback loop execution engine |
| P3 | orchestration | `src/orchestration/workflow_dsl/*.rs` | All | All have `#![allow(dead_code)]` | Complete workflow DSL or remove |
| P3 | ui | `src/ui/input_handler.rs` | 1 | `#![allow(dead_code)]` - incomplete implementation | Complete or remove |
| P3 | ui | `src/ui/sticky_bar.rs` | N/A | Hardcoded ANSI codes without terminal capability checks | Use terminfo or crossterm for capability detection |
| P3 | ui | `src/ui/tui/*.rs` | Multiple | Missing `#[cfg(feature = "tui")]` on internal files | Add feature gates |
| P3 | ui | `src/ui/animations.rs` | N/A | `columns` field never used | Remove unused field or implement feature |
| P3 | testing | `src/testing/contract_testing/stubs.rs` | N/A | `MockServer.start()` only sets `running=true` | Implement actual HTTP mock server |
| P3 | evolution | `src/evolution/telemetry.rs` | 227 | TODO - allocation profiling not implemented | Integrate DHAT or custom allocator |

---

## Issues by Category

### Security Vulnerabilities (8 issues)
1. Unknown tools allowed by default (P0)
2. Path traversal only checks for ".." (P0)
3. Empty allowed_paths allows all (P0)
4. Shell command bypass vectors (P0)
5. Easily bypassed YOLO forbidden ops (P1)
6. Bypassable path substring matching (P1)
7. Weak knowledge path validation (P2)
8. No path validation in vision/screen tools (P2)

### Async/Blocking Issues (6 issues)
1. `std::thread::spawn()` in checkpointing (P0)
2. `std::fs::create_dir_all()` in session_log (P0)
3. Blocking `read_recent()` function (P0)
4. Blocking stdin read in execution (P0)
5. Blocking editor.read_line() in interactive (P0)
6. `std::sync::RwLock` in async contexts (P1)

### Performance Issues (8 issues)
1. O(N) brute-force vector search (P1)
2. No HNSW indexing (P1)
3. Hardcoded 100k limit (P1)
4. No SIMD dot product (P1)
5. Full re-embedding on rebuild (P1)
6. Synchronous BM25 instead of embeddings (P1)
7. Unbounded cache growth (P2)
8. Unbounded history/patterns growth (P2)

### Stub/Non-Functional Implementations (22 issues)
Resource management, computer control, container simulation, and supervision are largely non-functional stubs that only log messages.

### Missing Persistence (6 issues)
1. LlmCache no disk persistence (P0)
2. Local-first no persistence (P0)
3. Analytics in-memory only (P1)
4. Carbon tracker in-memory only (P1)
5. Dashboard stats not persisted (P1)
6. No key rotation (P1)

---

## Recommended Fix Order

### Week 1: Critical Security & Blocking
1. Fix safety checker unknown tool handling
2. Fix path traversal validation
3. Fix all blocking I/O in async contexts
4. Fix shell command bypass vectors

### Week 2: Core Async & Performance
1. Replace all `std::sync::RwLock` with `tokio::sync::RwLock`
2. Implement HNSW indexing for vector store
3. Add SIMD dot product
4. Integrate LlmCache with persistence

### Week 3: Resource & Supervision
1. Implement actual resource management (not stubs)
2. Implement supervision restart/escalation
3. Add metrics uncommenting/fixing
4. Complete container runtime integration

### Week 4: Polish & Cleanup
1. Complete or remove WIP modules
2. Add comprehensive path validation
3. Implement key rotation
4. Clean up dead code warnings

---

## Risk Assessment

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Security bypass via unknown tools | Critical | High | Immediate fix required |
| Path traversal attacks | Critical | Medium | Fix within days |
| Async runtime blocking | High | High | Affects responsiveness |
| Data loss on restart | High | High | Add persistence |
| Performance degradation | Medium | High | Implement indexing |
| Resource exhaustion | Medium | Medium | Implement throttling |

---

---

## Verification & Gap Analysis (2026-03-14)

### 1. Verification of Critical Issues
We have performed a targeted audit of the Rust codebase to verify the P0 and P1 issues reported above.

*   **[P0] Swarm Execution Logic (task_runner.rs / swarm.rs):** VERIFIED.
    *   `task_runner.rs:496` pops a task from the swarm queue using `swarm.next_task()`.
    *   Subsequent calls to `swarm.assign_task(&task_id)` and `swarm.complete_task(...)` fail because these methods use `.find()` on the `task_queue`, which no longer contains the popped task. This causes task assignment and results to be dropped silently.
*   **[P0] Safety (checker.rs / path_validator.rs):** VERIFIED.
    *   `checker.rs` returns `Ok(())` for unknown tools after logging an error, failing to block execution.
    *   `path_validator.rs` relies on a simple `contains("..")` check and skips validation entirely if `allowed_paths` is empty.
*   **[P0] Async Integrity (execution.rs / session_log.rs):** VERIFIED.
    *   Blocking `std::io::stdin().read_line()` is called within the async executor.
    *   `std::fs` (synchronous) is used extensively for session logging and checkpointing.
*   **[P1] Local-First Cache TTL (local_first.rs):** VERIFIED.
    *   `current_timestamp()` returns milliseconds, but `is_expired()` compares this directly against `self.ttl` (documented and used as seconds). A 3600s (1h) TTL is currently treated as 3.6s.
*   **[P1] Encryption Durability (encryption.rs):** VERIFIED.
    *   `derive_key` falls back to a random ephemeral salt if `load_or_create_salt` fails. This prevents decryption of any data encrypted during that session once the process restarts.
*   **[P1] Resource Leaks (self_reference.rs):** VERIFIED.
    *   `code_cache: HashMap<String, CachedCode>` grows unbounded with no eviction policy or size limit.
*   **[P1] Non-Functional Stubs (computer.rs / container.rs):** VERIFIED.
    *   `MouseController` and `KeyboardController` are skeletons that only log messages.
    *   `Container` devops tools are simulations that track history in a `Vec` but do not invoke Docker/Podman.
*   **[P1] Orchestration (swarm.rs):** VERIFIED.
    *   Uses `std::sync::RwLock` in async context.
    *   `assign_task` marks tasks as `InProgress` but lacks an execution loop to actually run them.

### 2. Gap Analysis: Missing from Original Report
Our review identified several additional areas of concern not fully captured in the consolidated metrics:

*   **[P2] LLM Cache Configuration (cache.rs):** VERIFIED.
    *   `LlmCache::lookup` performs semantic similarity search even if `semantic_matching` is disabled in config.
    *   `cost_tracker.record_savings` is called unconditionally on cache hits, ignoring the `track_costs` configuration.
*   **[P2] Evolution Safety Weakness (evolution/mod.rs):** VERIFIED.
    *   `is_protected` uses `path_str.contains(p)` for protected path validation. This is overbroad (matching "src/evolutionary/" when "src/evolution/" is protected) and easily bypassed (e.g., using symlinks or relative paths that don't contain the literal substring).
*   **Monolithic Files (Code Quality):** 
    *   `src/agent/execution.rs` (4922 lines): Extremely high cyclomatic complexity in the agent state machine.
    *   `src/evolution/daemon.rs` (3074 lines): The core evolution engine is a massive single-file implementation, making it difficult to audit and maintain.
    *   `src/self_healing.rs` (2435 lines): Another monolithic implementation that should be modularized.
*   **LSP Integration Depth:** While `lsp_tools.rs` exists and is registered, it relies on a `OnceCell` pattern that may lead to deadlocks if multiple threads attempt to initialize the language server simultaneously during a burst of tool calls.

### 3. Recommendations for Remediation
1.  **Immediate**: Replace `std::sync::RwLock` with `tokio::sync::RwLock` in all async modules (`swarm.rs`, `parallel.rs`, `self_reference.rs`).
2.  **Security**: Fix `checker.rs` to return `anyhow::bail!` for unknown tools and implement strict canonical path validation in `path_validator.rs`.
3.  **Performance**: Implement an LRU cache for `SelfReferenceSystem::code_cache`.
4.  **Refactoring**: Break down `execution.rs` and `daemon.rs` into logical submodules (e.g., `agent::execution::states`, `evolution::daemon::discovery`).
5.  **Functionality**: Replace computer/container stubs with real implementations (e.g., `enigo` or `inputbot` for HID, `bollard` for Docker).

---

*Report compiled from analysis by 10 specialized agents and verified by Gemini CLI on 2026-03-14.*

---

## Independent Review Addendum (2026-03-14)

This addendum reflects a direct repository audit of the Rust sources plus a successful `cargo check`.

### A. Material Findings Missing From The Report

| Priority | Module | File | Lines | Missing Issue | Why It Matters |
|----------|--------|------|-------|---------------|----------------|
| **P0** | orchestration / agent | `src/agent/task_runner.rs`, `src/orchestration/swarm.rs` | `task_runner.rs:496-551`, `swarm.rs:1005-1043`, `swarm.rs:1046-1060` | **Swarm task lifecycle is internally inconsistent.** `next_task()` removes the task from `task_queue`, but `assign_task()` and `complete_task()` only operate on tasks still inside `task_queue`. The orchestrated runner pops a task first, then calls `assign_task(&task_id)`, which returns no agents; later `complete_task()` also cannot record completion for that popped task. | This breaks the main swarm execution path rather than merely leaving it "incomplete". The current implementation loses task state/results once a queued task is popped. |
| **P1** | session | `src/session/local_first.rs` | `31-35`, `107-108`, `160-169`, `1020-1026` | **TTL unit bug in local-first cache.** `current_timestamp()` returns milliseconds, but `ttl` is documented and used as seconds. `is_expired()` compares `created_at + ttl` directly, so `.with_ttl(3600)` expires in about 3.6 seconds, not one hour. | This makes cache retention and offline behavior materially wrong and invalidates the intended "1 hour TTL" policy. |
| **P1** | session | `src/session/encryption.rs` | `38-49`, `77-86` | **Salt persistence failure degrades into ephemeral encryption keys.** If the salt file cannot be read or written, `derive_key()` silently generates a random in-memory salt for that session. | Any data encrypted in that session becomes undecryptable after restart. This is a real durability bug and is more concrete than the report's broad "key rotation missing" note. |
| **P2** | session | `src/session/cache.rs` | `542-567`, `645-699` | **`LlmCacheConfig.semantic_matching` is ignored.** `lookup()` always performs embedding similarity search; there is no branch that disables semantic matching. | The public configuration surface does not match runtime behavior. Tests currently cover serialization/defaults, not behavioral enforcement of this flag. |
| **P2** | session | `src/session/cache.rs` | `551-568`, `691-693` | **`LlmCacheConfig.track_costs` is ignored.** Cost savings are recorded unconditionally during cache hits. | Reporting/analytics settings are misleading, and downstream stats can be wrong even when cost tracking is explicitly disabled. |
| **P2** | evolution | `src/evolution/mod.rs` | `46-54`, `231-235` | **Protected-path enforcement is substring-based.** `is_protected()` uses `path_str.contains(...)` against a short static list. | This is both bypass-prone for alternate path forms and overbroad for unrelated paths containing the same substring. The report mentions the static list, but not the concrete matcher defect. |

### B. Corrections / Severity Adjustments To Existing Report Items

1. **`src/orchestration/parallel.rs` is overstated in the current report.**  
   The cited `StdRwLock` at `src/orchestration/parallel.rs:958-999` protects `ExecutionStats.history`, a synchronous metrics buffer with no `.await` while the lock is held. That is not the same class of issue as the async coordination problems in `swarm.rs`.

2. **`src/cognitive/intelligence.rs` and `src/cognitive/self_improvement.rs` should not be treated as verified async-lock defects.**  
   Both files explicitly document that their current APIs are synchronous (`src/cognitive/intelligence.rs:57-60`, `src/cognitive/self_improvement.rs:16-19`). Keeping `std::sync::RwLock` there is a maintenance note, not a present async-runtime bug.

3. **The LSP `OnceCell` concern is not supported by the implementation as written.**  
   `src/tools/lsp_tools.rs:39-46` uses `tokio::sync::OnceCell::get_or_try_init`, which is designed for async lazy initialization. I did not find evidence in this code alone to justify the report's deadlock warning.

4. **The chat-store encryption note needs tighter wording.**  
   `src/session/chat_store.rs:111-119` now fails closed on decryption errors when encryption is enabled; it does not silently parse corrupted encrypted blobs as plaintext. The remaining issue is that encryption is optional when `EncryptionManager` is never initialized (`src/session/chat_store.rs:78-82`), which is a narrower claim than the report currently makes.

### C. Verification Notes

- `cargo check -q` completed successfully on 2026-03-14.
- The largest practical gaps I found were logic/integration bugs, not compilation failures.
- Existing tests do not appear to cover the popped-task swarm path or the local-first TTL unit mismatch.
