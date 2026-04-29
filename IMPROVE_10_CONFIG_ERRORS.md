# Selfware Configuration, Error Handling & Architecture Review

> Analysis of: `src/lib.rs`, `src/main.rs`, `src/config/*`, `src/errors.rs`, `src/concurrency.rs`, `src/agent/{learning,progress,tui_events,turn_artifacts,evolution_events}.rs`, `src/analysis/*`, `src/profiles/*`, `Cargo.toml`, `Cargo.lock` (head).

---

## 1. Config Parsing Bugs / Missing Validation

### 1.1 `rust-version = "1.91"` — crate is unbuildable on any existing toolchain
**File:** `Cargo.toml:13`  
**Severity:** 🔴 Build failure  
Rust 1.91 does not exist (latest stable as of 2026-04 is ~1.87–1.88). Cargo will refuse to build the crate on every currently available toolchain. Change to a released version (e.g. `1.78` or whatever the actual MSRV is).

### 1.2 `toml = "1.1"` — non-existent crate version
**File:** `Cargo.toml:76`  
**Severity:** 🔴 Build failure  
The `toml` crate latest stable is in the `0.8.x` range; there is no `1.1` release. `cargo` will fail to resolve this dependency. It should likely be `toml = "0.8"`.

### 1.3 `nix = "0.31"` — likely unavailable version
**File:** `Cargo.toml:128`  
**Severity:** 🟠 Build failure (likely)  
`nix` latest stable as of early 2025 is `0.29.0`. Version `0.31` probably does not exist yet. Verify and pin to a published version (e.g. `0.29`).

### 1.4 Silent parse failures for `SELFWARE_*` env overrides
**Files:** `src/config/loader.rs:329-337` (`MAX_TOKENS`), `:345-353` (`TEMPERATURE`), `:354-362` (`TIMEOUT`)  
**Severity:** 🟡 Config bug  
All three env overrides use `if let Ok(n) = var.parse::<T>()` — if the user sets `SELFWARE_MAX_TOKENS=abc`, the parse fails silently and the config keeps the old value with **no warning**. This is a silent misconfiguration trap. Use `else` branches to emit at least an `eprintln!` warning.

### 1.5 `temperature` and `animation_speed` validation accept `NaN`/`Inf`
**File:** `src/config/validation.rs:64-75`, `:112-124`  
**Severity:** 🟡 Logic bug  
`f32` NaN/Inf comparisons return `false`, so `temperature = NaN` passes both `< 0.0` and `> 10.0` checks undetected. Same for `animation_speed`. Add `!value.is_finite()` checks.

### 1.6 `ConcurrencyConfig::validate()` is bypassed for programmatic construction
**File:** `src/concurrency.rs:50-55`  
**Severity:** 🟡 Validation gap  
`ConcurrencyGovernor::from_config` clamps values with `.max(1)` but never calls `ConcurrencyConfig::validate()`. If a user builds `ConcurrencyConfig` directly (not via TOML/env), values `> 256` are silently truncated instead of rejected. The clamp is a safety net, but the validator should still be invoked so errors are explicit.

---

## 2. Error Handling Anti-Patterns

### 2.1 `expect` in library constructor (`AutoConfigurator::new`)
**File:** `src/config/auto_config.rs:72`  
**Severity:** 🔴 Runtime panic  
```rust
.client = reqwest::Client::builder()...build().expect("Failed to build HTTP client");
```
This is library code called from `unpack.rs`. If the HTTP client fails to build (TLS misconfiguration, missing certs, memory pressure), the **entire process aborts** instead of returning a clean `Result`. Return `Result<Self>` or `anyhow::Context`.

### 2.2 `expect` in signal handler registration
**File:** `src/main.rs:47`  
**Severity:** 🟠 Runtime panic  
```rust
let mut sigterm = tokio::signal::unix::signal(...).expect("failed to register SIGTERM handler");
```
On Unix systems with restricted signal resources (e.g. containers with tight ulimits), registration can fail. The process should fall back to `ctrl_c` only, not abort.

### 2.3 `unwrap()` in cycle detection
**File:** `src/analysis/code_graph.rs:507`  
**Severity:** 🟠 Runtime panic  
```rust
let cycle_start = path.iter().position(|x| x == &edge.target).unwrap();
```
If the DFS logic drifts due to a future graph mutation bug, this `unwrap()` panics. Replace with `.expect("cycle consistency")` or safe handling.

### 2.4 `RwLock` poisoning ignored throughout vector store
**File:** `src/analysis/vector_store.rs` (repeated pattern, e.g. `:612`, `:623`, `:627`, `:666`, etc.)  
**Severity:** 🟡 Data corruption risk  
Every `RwLock` guard uses `.unwrap_or_else(|e| e.into_inner())`, which **recovers from poisoning and proceeds with potentially corrupt data**. In a concurrent embedding provider (`TfIdfEmbeddingProvider`), a panicked writer can leave the vocabulary in an inconsistent state; subsequent readers will use it unchecked. Poisoning should be treated as a fatal error or the data structure should be re-initialized.

---

## 3. Concurrency Bugs

### 3.1 `StderrProgressEmitter` double-lock ordering with `OUTPUT_LOCK`
**File:** `src/agent/progress.rs:310-316`  
**Severity:** 🟠 Potential deadlock  
```rust
let _global = crate::output::OUTPUT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
let _local = self.lock.lock().unwrap_or_else(|e| e.into_inner());
```
If any other code path locks `OUTPUT_LOCK` while holding a progress emitter lock (or vice versa), this is a lock-order inversion. Since `OUTPUT_LOCK` is a global static and `self.lock` is per-emitter, any cross-caller emission can deadlock. Fix: always acquire locks in a consistent global-to-local order, or use a single lock.

### 3.2 `ThroughputTracker::snapshot` mixes `Mutex` with atomics without clear ordering
**File:** `src/agent/evolution_events.rs:194-219`  
**Severity:** 🟡 Race / inaccurate metrics  
`last_snapshot` is a `Mutex<Instant>`, while counters are `AtomicU64`. Another thread can update `tokens_in` between the `load` and the `swap`, causing the delta calculation to occasionally under-report rates. The atomic loads/swap should happen inside the mutex critical section, or the whole snapshot should be atomic.

### 3.3 `evolution_events.rs` uses `std::sync::Mutex` inside async `EvolutionBus`
**File:** `src/agent/evolution_events.rs:146`, `snapshot()`  
**Severity:** 🟡 Async blocking  
`ThroughputTracker` stores a `std::sync::Mutex<Instant>`. When `snapshot()` is called from an async context (e.g. the tokio timer loop), it can block the executor thread. Replace with `tokio::sync::Mutex` or compute deltas lock-free with atomics only.

---

## 4. Library Architecture Issues

### 4.1 Orphan modules: `vector_index.rs` and `vector_query.rs` are not in the module tree
**File:** `src/analysis/mod.rs` (missing entries)  
**Severity:** 🟠 Dead code / drift  
`src/analysis/mod.rs` lists:
```rust
pub mod analyzer;
pub mod bm25;
pub mod code_graph;
pub mod tech_debt;
pub mod tier_allocator;
pub mod vector_store;
pub mod workspace_graph;
```
It **does not** register `vector_index` or `vector_query`. `vector_index.rs` even contains a comment: *"NOTE: This module is not currently registered in the crate module tree."* These files compile only if referenced elsewhere (they are not). They duplicate types (`VectorIndex`, `SearchFilter`) that also exist in `vector_store.rs`, creating a maintenance hazard. Either register them or delete them.

### 4.2 `syn` declared as a direct dependency
**File:** `Cargo.toml:126`  
**Severity:** 🟡 Compile-time bloat  
```rust
syn = { version = "2.0", features = ["full", "parsing"] }
```
`syn` is a proc-macro crate. A binary/library crate that does not define proc macros should not pull it in directly. It adds significant compile time and is likely unused (or should be moved to a build-dependency / dev-dependency). Verify usage; if none, remove.

### 4.3 `kv_store` is feature-gated but `tokens` module is not
**File:** `src/lib.rs:152-153`, `:161`  
**Severity:** 🟡 Inconsistency  
```rust
#[cfg(feature = "tokens")]
pub mod kv_store;
// ...
pub mod tokens;   // NOT gated
```
If `--no-default-features` is used, `kv_store` disappears but `tokens` remains. This is confusing and suggests the feature flag boundary is not clean.

### 4.4 `Config` exposes a `pub` field with a `pub(crate)` type
**File:** `src/config/mod.rs:180`  
**Severity:** 🟡 API ergonomics  
```rust
pub cache: crate::session::cache::LlmCacheConfig;
```
`session` is `pub(crate)`. External crates using `selfware` as a library can see the `cache` field exists but cannot name its type (`LlmCacheConfig`) to construct or pattern-match on `Config`. Either re-export the type at the crate root or make the field `pub(crate)`.

---

## 5. Feature Flag Misconfiguration

### 5.1 `browser` feature is empty but module is gated; `swebench` is always compiled
**File:** `Cargo.toml:173`, `src/lib.rs:143-146`  
**Severity:** 🟡 Inconsistency  
```rust
// Cargo.toml
browser = []
// src/lib.rs
#[cfg(feature = "browser")]
pub mod browser;
pub mod swebench;   // always compiled
```
The `swebench` module has examples that require `bench-harness`, yet the module itself is unconditional. This means `--no-default-features` still compiles SWE-bench code. Decide whether `swebench` should be behind `bench-harness` or another flag.

### 5.2 `hot-reload` feature is an empty stub
**File:** `Cargo.toml:152-153`  
**Severity:** 🟡 Dead feature  
```rust
hot-reload = []
```
There is no code guarded by `#[cfg(feature = "hot-reload")]` in the read files. Either implement the feature or remove the flag to avoid confusing users.

---

## 6. Dependency Issues

### 6.1 `reqwest = "0.13"` — verify existence
**File:** `Cargo.toml:57`  
**Severity:** 🟠 Potential build failure  
As of early 2025, `reqwest` latest is `0.12.x`. If `0.13` has not been published by the build date, this will fail. Pin to a confirmed version (`0.12`) or confirm `0.13` is available.

### 6.2 `bincode = "2.0"` depends on a not-yet-stable major rewrite
**File:** `Cargo.toml:100`  
**Severity:** 🟠 Potential build failure  
`bincode` 2.0 was in release-candidate state for a long time. The code uses `bincode::serde::encode_to_vec` / `decode_from_slice` (the 2.0 API). If crates.io does not have a stable `2.0`, this will not resolve. Consider pinning to the latest RC or falling back to `1.3` with the legacy API.

### 6.3 OpenTelemetry version skew
**File:** `Cargo.toml:120-125`  
**Severity:** 🟡 Runtime incompatibility  
```toml
opentelemetry = "0.21.0"
opentelemetry_sdk = { version = "0.21.0", features = ["rt-tokio"] }
opentelemetry-otlp = "0.14.0"
tracing-opentelemetry = "0.22.0"
```
The OTel Rust ecosystem is notoriously brittle across minor versions. `0.21` + `0.14` + `0.22` may contain ABI/metric-format mismatches that cause silent export failures or runtime panics. Upgrade to a single compatible version set (e.g. OTel `0.27` ecosystem) and test the OTLP exporter.

### 6.4 `llmfit-core` git dependency with no fallback
**File:** `Cargo.toml:111`  
**Severity:** 🟡 Supply-chain risk  
```toml
llmfit-core = { git = "https://github.com/AlexsJones/llmfit", rev = "f216779" }
```
If the repository is deleted, made private, or the revision is force-pushed, all builds break permanently. Vendor the code, publish a fork to crates.io, or add a `[patch]` fallback.

---

## 7. Runtime Panics / Crash Vectors

### 7.1 `is_multiple_of` requires Rust 1.91, which doesn't exist
**File:** `src/agent/learning.rs:195`  
**Severity:** 🔴 Compile error (secondary to 1.1)  
```rust
if step > 0 && step.is_multiple_of(5) {
```
`usize::is_multiple_of` is a relatively recent std addition. Because the MSRV is set to `1.91`, this is assumed available, but since `1.91` cannot be installed, the crate cannot be compiled. If you lower the MSRV, verify this method exists in your target version or replace with `step % 5 == 0`.

### 7.2 `VectorStore::collection()` uses `unreachable!` after insertion
**File:** `src/analysis/vector_store.rs:1533`  
**Severity:** 🟡 Panic vector  
```rust
.unwrap_or_else(|| unreachable!("collection was just inserted"))
```
While logically correct today, if the hash map logic is ever changed (e.g. custom hasher, deferred insertion), this becomes an instant abort. Use `expect("collection insert")` or return `Result`.

### 7.3 `is_tui_active()` may be missing when `tui` feature is disabled
**File:** `src/agent/progress.rs:304`  
**Severity:** 🟠 Build failure (`--no-default-features`)  
```rust
if crate::output::is_tui_active() {
```
The `tui` feature is in `default`, but building with `--no-default-features` removes the TUI module. If `is_tui_active()` is defined inside the `tui` feature gate, this line will not compile. Either gate this block with `#[cfg(feature = "tui")]` or ensure `is_tui_active()` exists unconditionally.

### 7.4 `save_unpack_config` writes `config.agent.step_timeout_secs` without type validation
**File:** `src/config/unpack.rs:670-702`  
**Severity:** 🟡 TOML injection risk  
The auto-generated TOML string is built with `format!(...)` directly embedding boolean/numeric values. While not a security vulnerability for local use, if `endpoint` or `model` ever contains `"` characters, the generated TOML will be malformed. Use a TOML serializer (`toml::to_string`) instead of raw string interpolation.

---

## Summary Table

| # | Issue | File | Line | Severity |
|---|-------|------|------|----------|
| 1 | `rust-version = "1.91"` (non-existent) | `Cargo.toml` | 13 | 🔴 Build |
| 2 | `toml = "1.1"` (non-existent) | `Cargo.toml` | 76 | 🔴 Build |
| 3 | `nix = "0.31"` (likely non-existent) | `Cargo.toml` | 128 | 🟠 Build |
| 4 | Silent env var parse failures | `src/config/loader.rs` | 329-362 | 🟡 Config |
| 5 | `NaN`/`Inf` passes temp/animation validation | `src/config/validation.rs` | 64-124 | 🟡 Config |
| 6 | `expect` in `AutoConfigurator::new` | `src/config/auto_config.rs` | 72 | 🔴 Panic |
| 7 | `expect` in SIGTERM handler | `src/main.rs` | 47 | 🟠 Panic |
| 8 | `unwrap()` in cycle detection | `src/analysis/code_graph.rs` | 507 | 🟠 Panic |
| 9 | RwLock poisoning ignored (vector store) | `src/analysis/vector_store.rs` | 612+ | 🟡 Corruption |
| 10 | Double-lock deadlock risk in progress emitter | `src/agent/progress.rs` | 310-316 | 🟠 Deadlock |
| 11 | Orphan modules (`vector_index`, `vector_query`) | `src/analysis/mod.rs` | — | 🟠 Arch |
| 12 | `syn` as direct dependency | `Cargo.toml` | 126 | 🟡 Bloat |
| 13 | `is_tui_active` build failure without `tui` | `src/agent/progress.rs` | 304 | 🟠 Build |
| 14 | OTel version skew | `Cargo.toml` | 120-125 | 🟡 Runtime |
| 15 | `bincode = "2.0"` stability | `Cargo.toml` | 100 | 🟠 Build |
| 16 | `llmfit-core` git-only dep | `Cargo.toml` | 111 | 🟡 Supply |
| 17 | Raw string TOML generation | `src/config/unpack.rs` | 670-702 | 🟡 Data |
| 18 | `is_multiple_of` MSRV mismatch | `src/agent/learning.rs` | 195 | 🔴 Compile |
