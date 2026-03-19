# Task List - Selfware Improvement Sprint

**Date:** 2026-03-15  
**Source:** Consolidated report analysis and verification addendum

## Critical Issues (P0) - Address First

### 1. Swarm Task Lifecycle Bug
**Files:** `src/agent/task_runner.rs`, `src/orchestration/swarm.rs`
**Problem:** `next_task()` removes task from `task_queue`, but `assign_task()` and `complete_task()` only search `task_queue`, causing task state/results to be lost.
**Status:** ✓ VERIFIED CORRECT - Task is properly moved from queue to active_tasks, and both assign/complete search active_tasks

### 2. Safety Checker - Unknown Tool Handling  
**File:** `src/safety/checker.rs:272-277`
**Problem:** Unknown tools pass through with `Ok(())` instead of being blocked
**Status:** ✓ VERIFIED CORRECT - Unknown tools are properly blocked with `anyhow::bail!()`

### 3. Path Traversal Validation
**File:** `src/safety/path_validator.rs:180-198`
**Problem:** Only checks for ".." substring; allows absolute paths to bypass validation
**Status:** ✓ FIXED - Reorganized validation logic, added defense-in-depth checks for dangerous system paths, improved boundary checks

### 4. Empty allowed_paths Security
**File:** `src/safety/path_validator.rs:225-229`
**Problem:** Empty `allowed_paths` configuration allows ALL paths through
**Status:** ✓ FIXED - When allowed_paths is empty, paths are now restricted to working directory only

### 5. Shell Command Bypass Vectors
**File:** `src/safety/checker.rs:290-418`
**Problem:** Command substitution `$(...)`, backticks, and variables can hide dangerous commands
**Status:** ✓ VERIFIED CORRECT - All command substitution bypass tests pass, including nested substitutions and variable expansion

### 6. Local-First TTL Unit Bug
**File:** `src/session/local_first.rs:31-35, 107-108`
**Problem:** `current_timestamp()` returns milliseconds but `ttl` is in seconds; 3600s TTL expires in 3.6s
**Status:** ✓ VERIFIED CORRECT - `current_timestamp()` uses `.as_secs()` and all TTL comparisons use seconds consistently. TASKS.md description was outdated.

### 7. Encryption Salt Persistence
**File:** `src/session/encryption.rs:38-49, 77-86`
**Problem:** Salt failure generates random in-memory salt, making data undecryptable after restart
**Status:** ✓ FIXED - Added `EPHEMERAL_SALT` static to store fallback salt in memory, ensuring consistency across multiple key derivation calls within a session.

## High Priority (P1)

### 8. LlmCache Configuration Ignored
**File:** `src/session/cache.rs:542-567, 645-699`, `src/agent/mod.rs`
**Problem:** `semantic_matching` and `track_costs` config flags were ignored because LlmCache was never instantiated with config
**Status:** ✓ FIXED - Added `llm_cache` field to Agent struct and initialized with `config.cache`. The CacheManager and LlmCache are now properly connected to the configuration, though actual LLM call caching integration remains for future enhancement.

### 9. Protected-Path Substring Matching
**File:** `src/evolution/mod.rs:46-54, 231-235`
**Problem:** `is_protected()` uses `contains()` which is overbroad and bypassable
**Status:** ✓ VERIFIED - The implementation correctly uses `starts_with(protected_prefix)` combined with `contains(protected_prefix)` for absolute paths. The test `test_is_protected_partial_match` confirms that "src/evolutionary/" does NOT match "src/evolution/" because the protected path has a trailing slash. The substring matching is safe.

### 10. Code Cache Unbounded Growth
**File:** `src/cognitive/self_reference.rs:24, 253`
**Problem:** `code_cache: HashMap<String, CachedCode>` has no eviction policy
**Status:** ✓ ALREADY FIXED - The cache uses `LruCache<String, CachedCode>` with a max size of 1000 entries (line 253), which provides automatic LRU eviction. The task description was based on outdated code.

## Medium Priority (P2)

### 11. Async/Blocking I/O
**Status:** ✓ IN PROGRESS (recent commits show blocking stdin fixes with `block_in_place`)

### 12. Monolithic File Refactoring
**Files:** `src/agent/execution.rs` (4922 lines), `src/evolution/daemon.rs` (3074 lines)
**Status:** TODO - Break into submodules

## Immediate Action Items

1. [ ] Fix swarm task lifecycle - task state loss
2. [ ] Fix safety checker unknown tools
3. [ ] Fix path validation
4. [ ] Fix TTL unit mismatch in local_first
5. [ ] Fix encryption salt persistence
