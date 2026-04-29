# IMPROVE-09: Infrastructure & Services Analysis

**Scope:** LSP client, MCP integration, hooks, session management, self-healing, observability, resource management, and related supervision/devops/computer modules.

**Date:** 2026-04-28

---

## 1. LSP Client (`src/lsp/`)

### Summary
JSON-RPC LSP client over stdio. Lazy-spawns one connection per `Language`. Self-hosted LSP server backed by a local symbol index.

### Critical Bugs
| Issue | Location | Impact |
|-------|----------|--------|
| `request()` timeout hardcoded to 30s | `client.rs` | No configurability; hangs on slow language servers |
| `did_change()` hardcodes document `version: 2` | `client.rs` | Violates LSP protocol; incremental sync will fail on subsequent edits |
| `dispatch_message()` silently drops all notifications except `textDocument/publishDiagnostics` | `client.rs` | Misses `window/showMessage`, `window/logMessage`, server-initiated `workspace/applyEdit` |
| `file_uri()` best-effort relative path without URI encoding | `client.rs` | Spaces, Unicode, and special characters produce invalid `file://` URIs |
| `shutdown()` aborts reader handle after 500ms sleep | `client.rs` | Non-compliant LSP shutdown; may truncate in-flight messages |
| No `didClose` implementation | `client.rs` | Server retains stale document state indefinitely |
| `load_intelligence()` constructs new `ProjectIntelligence` and calls `.refresh()` on every handler invocation | `server.rs` | No caching; every hover/definition request re-indexes the project |
| `symbol_range()` hardcodes end `character: 200` | `server.rs` | Symbol ranges are wildly inaccurate for long lines |
| `extract_token_at()` mixes `char_indices()` and `chars().collect::<Vec<_>>()` | `server.rs` | Unicode offsets drift; performance penalty from redundant collection |

### Recommended Fixes
1. Track document version per URI in an `Arc<RwLock<HashMap<Uri, u32>>>`.
2. Implement full `shutdown`/`exit` handshake per LSP spec.
3. Cache `ProjectIntelligence` across requests with an LRU or TTL eviction policy.
4. Use `lsp_types::Url` (or `url::Url`) for URI construction instead of string concatenation.

---

## 2. MCP Integration (`src/mcp/`)

### Summary
MCP stdio transport with `Content-Length` framing. Client lifecycle (connect, initialize, list_tools, call_tool). Server exposes Selfware `ToolRegistry` and hardcoded project resources. Discovery module wraps remote MCP tools as native `Tool` implementations.

### Critical Bugs
| Issue | Location | Impact |
|-------|----------|--------|
| `shutdown()` sends non-standard `notifications/shutdown` then kills after 500ms | `transport.rs` | Violates MCP spec; no graceful exit |
| `request()` timeout hardcoded to 60s; no retry logic | `transport.rs` | Transient stdio stalls become permanent failures |
| Reader task aborted on shutdown | `transport.rs` | May drop unconsumed responses/requests |
| Capabilities only advertises `roots.listChanged: false`; missing `sampling`, `experimental` | `client.rs` | Server may reject connection or disable advanced features |
| `call_tool()` extracts only `type: "text"` content blocks | `client.rs` | Discards images, resources, embedded binaries from tool results |
| `handle_tools_call()` stringifies all tool results via `as_str()` or `to_string_pretty()` | `server.rs` | Loses structured data; nested JSON becomes escaped strings |
| Resource list hardcoded to 3 URIs | `server.rs` | Cannot expose dynamic project files to MCP clients |
| Path traversal guard in `read_project_file()` uses `canonicalize()`, which fails if file does not yet exist | `server.rs` | Breaks creation of new files via MCP |
| No handlers for `sampling/createMessage` or `roots/list` | `server.rs` | Incomplete MCP server implementation |

### Recommended Fixes
1. Extract a shared `Content-Length` framing library used by both LSP and MCP to eliminate duplication.
2. Parse all MCP content block types (`text`, `image`, `resource`, `embedded_resource`).
3. Replace `canonicalize()`-based traversal guard with a simple prefix check against the project root.
4. Implement `sampling/createMessage` and `roots/list` handlers or return `METHOD_NOT_FOUND` explicitly.

---

## 3. Hooks (`src/hooks/`)

### Summary
Event-driven hook registry (`PreToolUse`, `PostToolUse`, `Stop`). Shell execution backend.

### Critical Bugs
| Issue | Location | Impact |
|-------|----------|--------|
| `fire()` returns immediately on first `HookAction::Skip` without executing remaining hooks | `mod.rs` | Later hooks (e.g., audit logging) never run |
| Placeholder expansion only supports `{path}` and `{tool}`; `{description}` advertised but not implemented | `mod.rs` | Documentation/behaviour mismatch |
| `extract_path_from_args()` only looks for top-level JSON key `"path"` | `mod.rs` | Misses nested paths (e.g., `{"file": {"path": "..."}}`) |
| `run_shell_command()` invokes `sh -c` with no working directory or environment variable control | `shell_handler.rs` | Hooks run in unpredictable cwd; cannot inject env vars |
| Output capped at 64 KB per stdout/stderr stream | `shell_handler.rs` | Large outputs truncated silently |
| Timeout minimum is 1s (`hook.timeout_secs.max(1)`) | `shell_handler.rs` | Fast hooks still incur 1-second penalty |

### Recommended Fixes
1. Execute all hooks even when one returns `Skip`; collect actions and apply the most restrictive.
2. Support nested JSON path extraction or use a JSONPath query.
3. Allow `cwd` and `env` overrides in `HookConfig`.

---

## 4. Session Management (`src/session/`)

### Summary
Persistent encrypted chat storage, tool/LLM caches, task checkpointing with delta compression, undo/redo timeline.

### Critical Bugs
| Issue | Location | Impact |
|-------|----------|--------|
| `std::sync::RwLock` (blocking) inside async code for both `ToolCache` and `LlmCache` | `cache.rs` | Can block the tokio runtime worker threads |
| `is_cacheable()` / `invalidates_cache()` hardcode tool name strings | `cache.rs` | Fragile; new tools bypass cache logic silently |
| `LlmCache::lookup()` takes embeddings but no evidence of actual embedding generation pipeline | `cache.rs` | Semantic cache is wired but possibly unpowered |
| Relies on global `EncryptionManager::get()` singleton | `chat_store.rs` | Hard to test; hidden global state |
| Name sanitization collapses all non-alphanumeric chars to `_` | `chat_store.rs` | `file-a.rs` and `file_a.rs` become indistinguishable |
| `CheckpointEnvelope::get_hmac_key()` uses `rand::rng()` API | `checkpoint.rs` | Sensitive to `rand` crate version compatibility |
| HMAC key persisted to disk but not encrypted | `checkpoint.rs` | Relies solely on Unix file permissions (`0o600`) |
| `compute_delta()` returns `None` if vectors shrank, forcing full rewrite | `checkpoint.rs` | Unnecessary disk I/O on message truncation |
| `save_with_retry()` uses `std::thread::sleep` | `checkpoint.rs` | Blocks async runtime during retry backoff |
| `FileSnapshot` stores full file content in memory per checkpoint | `edit_history.rs` | Default `max_checkpoints = 100` can balloon RAM |

### Recommended Fixes
1. Replace `std::sync::RwLock` with `tokio::sync::RwLock` in all async cache paths.
2. Encrypt the HMAC key with the same AES-256-GCM key used for chat storage.
3. Use `tokio::time::sleep` (or spawn_blocking) in `save_with_retry`.
4. Store edit deltas (e.g., diff patches) instead of full file snapshots in `edit_history`.

---

## 5. Self-Healing (`src/self_healing/`)

### Summary
Auto-recovery with error pattern learning, recovery strategy execution, state checkpointing, health prediction, and escalation chains.

### Critical Bugs
| Issue | Location | Impact |
|-------|----------|--------|
| `execute_retry()` uses `tokio::task::block_in_place` + `std::thread::sleep` | `executor.rs` | Blocks a tokio worker for up to 30s per recovery |
| `execute()` / `execute_internal()` are entirely synchronous | `executor.rs` | Cannot be called from async code without blocking |
| `HealthPredictor::update_prediction()` uses naive heuristics with hardcoded thresholds | `mod.rs` | Predictions are not statistically robust |
| `ErrorLearner::detect_patterns()` groups by exact string match (`error_type:context`) | `mod.rs` | Misses similar errors with slightly different messages |
| `StateManager::checkpoint()` stores state only in memory (`VecDeque`) | `mod.rs` | Checkpoints are lost on process restart |
| `RecoveryExecutor::execute_action()` — `Restart` action only restores checkpoint, does not actually restart any component | `executor.rs` | Misleading action name; no process/container restart |
| `RecoveryAction::Custom` — unknown actions treated as no-op signals | `executor.rs` | Silent swallowing of typos or unimplemented handlers |
| `SelfHealingEngine::handle_error()` returns `Option<RecoveryExecution>`; caller cannot distinguish "disabled" from "no strategy found" | `mod.rs` | Ambiguous API contract |

### Recommended Fixes
1. Convert `RecoveryExecutor` to async (`async fn execute(...)`) and use `tokio::time::sleep`.
2. Persist `StateManager` checkpoints to disk (re-use `session::checkpoint` infrastructure).
3. Implement fuzzy/error-tolerant pattern matching (e.g., Levenshtein on error messages).
4. Rename `Restart` action to `RestoreCheckpoint` or add actual process restart logic.

---

## 6. Observability (`src/observability/`)

### Summary
Analytics, carbon tracking, dashboard, log analysis, telemetry, test dashboard. Several modules are marked `EXPERIMENTAL` or `#[allow(dead_code)]`.

### Critical Bugs
| Issue | Location | Impact |
|-------|----------|--------|
| `AnalyticsDashboard` uses `std::sync::RwLock` and silently ignores errors | `analytics.rs` | Lock poisoning is invisible; data may be stale |
| `LogLevel::from_str()` returns `Self` (not `Result`), defaulting to `Info` on unknown input | `log_analysis.rs` | Misspelled log levels silently become `Info` |
| `RootCauseAnalyzer::analyze()` deduplicates by `format!("{:?}", cause.category)` | `log_analysis.rs` | Fragile string key; format changes break dedup |
| `AnomalyDetector` stores `error_baseline` as `f32` inside `std::sync::RwLock` | `log_analysis.rs` | Same blocking issue; precision loss from `f32` |
| `init_tracing_with_filter()` silently ignores OpenTelemetry initialization failures | `telemetry.rs` | OTel exporter misconfigurations go unnoticed |
| Counter-based sampling (`should_sample()`) can skew if rate changes mid-stream | `telemetry.rs` | Non-uniform sampling distribution |
| `TRACING_GUARD` stored in `OnceLock<Mutex<Option<...>>>` | `telemetry.rs` | Potential background writer leak if init fails partially |
| Module comment marks `dashboard.rs` as `EXPERIMENTAL — no call sites, candidate for removal` | `dashboard.rs` | Dead code risk |

### Recommended Fixes
1. Replace `std::sync::RwLock` with `tokio::sync::RwLock` in async contexts.
2. Change `LogLevel::from_str` to return `Result<Self, ParseError>`.
3. Log OpenTelemetry init errors at `error!` level instead of swallowing.
4. Use atomic random sampling (e.g., `rand::random::<f64>() < rate`) instead of counter modulo.

---

## 7. Resource Management (`src/resource/`)

### Summary
Resource manager coordinating GPU, memory, disk, and adaptive quotas.

### Critical Bugs
| Issue | Location | Impact |
|-------|----------|--------|
| `monitor_loop()` infinite loop with no cancellation token | `mod.rs` | Task leaks on shutdown; cannot be gracefully stopped |
| `get_resource_pressure()` only considers memory and GPU memory ratios; ignores CPU and disk | `mod.rs` | CPU-bound or disk-full scenarios not detected |
| `handle_pressure()` awaits async subcalls but no backpressure mechanism | `mod.rs` | Can spawn unbounded concurrent throttling tasks |
| `get_models_size()` returns hardcoded 10 GB placeholder | `disk.rs` | Misleading disk pressure calculations |
| `cleanup_orphaned_files()` is empty stub | `disk.rs` | Orphaned checkpoints/logs accumulate indefinitely |
| Default paths (`./checkpoints`, `./logs`, `./models`) are relative to CWD | `disk.rs` | Breaks when launched from unexpected directories |
| `throttle_compute()` multiplies current `AtomicU32` throttle value by factor; logic treats throttle as raw count | `gpu.rs` | Repeated calls collapse throttle to near-zero |
| `reduce_batch_size()` is empty stub | `gpu.rs` | No actual batch reduction under GPU pressure |
| `get_usage()` maxes utilization and temperature across GPUs | `gpu.rs` | Single hot GPU masks healthy ones; no per-GPU reporting |
| `allocate()` uses `fetch_add` but never checks against actual system memory limits | `memory.rs` | Can overcommit memory without OS feedback |
| `free()` uses `fetch_sub` (can underflow/wrap; not `saturating_sub`) | `memory.rs` | Double-free or accounting bug causes underflow |
| Memory action handler is spawned as detached task | `memory.rs` | No confirmation actions completed; no backpressure |
| `AdaptiveQuotas::check()` derives system-memory limit from `max_context_tokens * 100` | `quotas.rs` | Arbitrary conversion; unrelated concepts coupled |

### Recommended Fixes
1. Add a `CancellationToken` to `monitor_loop()` and all health-check loops.
2. Integrate CPU load (via `sysinfo`) and disk usage into `get_resource_pressure`.
3. Use XDG dirs or a configurable base directory instead of CWD-relative paths.
4. Replace `fetch_sub` with `fetch_sub(...).saturating_sub(...)` or track allocation with an `AtomicI64`.
5. Decouple token limits from memory quotas in `AdaptiveQuotas::check()`.

---

## 8. Supervision (`src/supervision/`)

### Summary
Erlang-style supervisor with `OneForOne`/`OneForAll`/`RestForOne` strategies, circuit breaker, and health checks.

### Critical Bugs
| Issue | Location | Impact |
|-------|----------|--------|
| `Supervisor::start()` spawns a task but returns immediately; no way to await readiness | `mod.rs` | Race between `start()` and sending `ChildEvent` |
| `Supervisor` stores `children: Vec<ChildSpec>` but `ChildSpec` contains `Arc<dyn ComponentFactory>` without `Debug` | `mod.rs` | Debugging is limited to `has_factory: bool` |
| `CircuitBreaker::call()` uses `Ordering::Relaxed` for state and counter reads | `circuit_breaker.rs` | Potential torn reads on some architectures; state/counter desync |
| `CircuitBreaker::transition_to()` resets both `failure_count` and `success_count` to zero | `circuit_breaker.rs` | Metrics lost on every state transition |
| `HealthMonitor::start()` has an infinite `loop` with no cancellation | `health.rs` | Cannot stop health checks gracefully |
| `HealthMonitor` `_failure_threshold` is prefixed with underscore (unused) | `health.rs` | Config field is wired but not enforced |
| `GpuHealthCheck` only checks GPU 0 | `health.rs` | Multi-GPU systems only monitor one device |
| `start_health_endpoint()` responds `200 OK` to **any** request regardless of path | `health.rs` | Returns OK for `/anything`, not just `/health` |

### Recommended Fixes
1. Add `CancellationToken` to `HealthMonitor` and `Supervisor` loops.
2. Use `Ordering::SeqCst` (or at least `Acquire`/`Release`) for circuit breaker state transitions.
3. Return `404` for non-`/health` paths in the health endpoint, or use a lightweight HTTP framework.
4. Iterate over all GPU devices in `GpuHealthCheck`.

---

## 9. DevOps / Computer Automation (`src/devops/`, `src/computer/`)

### Summary
Background process manager with health checks and auto-restart. Docker/Podman abstraction. Keyboard, mouse, screen, and window automation.

### Critical Bugs
| Issue | Location | Impact |
|-------|----------|--------|
| `start()` drops the reserved `TcpListener` immediately before `cmd.spawn()` | `process_manager.rs` | Race window for port theft between drop and child bind |
| Health check loop polls every 100ms holding `processes.read().await` | `process_manager.rs` | Contended lock; blocks `stop()`, `restart()`, `get()` |
| `restart()` removes old entry before starting fresh, losing log history | `process_manager.rs` | Post-mortem debugging impossible |
| All container lifecycle methods only update internal `HashMap` caches | `container.rs` | **Entirely simulated** — no Docker/Podman subprocess or API calls |
| `RuntimeType::Auto` hardcodes to `"docker"` without probing PATH | `container.rs` | Fails on Podman-only systems |
| `validate_image_name()` rejects `'@'` (digest separator) but allows `':'` | `container.rs` | Digest references rejected; tag references accepted |
| `Image::parse_reference()` heuristic fails on registry ports like `localhost:5000/image` | `container.rs` | Mis-parses port as tag |
| `type_text()` with non-zero delay spawns one external process per character on WSL | `keyboard.rs` | Extreme overhead for long strings |
| `build_sendkeys_combo()` maps `super/meta/cmd` to `^` (Ctrl) on Windows | `keyboard.rs` | Incorrect modifier mapping |
| `capture_region()` captures full screen and returns it uncropped | `screen.rs` | Region parameter is ignored |
| macOS window functions are stubs (log warning, return `Ok(())`) | `window.rs` | Non-functional on macOS |

### Recommended Fixes
1. Actually spawn `docker`/`podman` CLI commands or use the Docker API crate (`bollard`).
2. Fix `Image::parse_reference` to handle registry ports (e.g., split on last `:` after last `/`).
3. Implement macOS window management via `CGWindow` / AppleScript, or return a proper `NotImplemented` error.
4. Use `xdotool` type with a delay argument instead of per-character process spawning.

---

## 10. Skills (`src/skills/mod.rs`)

### Summary
Skill registry loading markdown files with YAML frontmatter.

### Key Issues
- `discover_dir()` only searches **one level deep** for `*.md`.
- No hot-reload; skills are loaded once at startup.
- No validation of required frontmatter fields.

### Recommended Fixes
1. Support recursive discovery (`walkdir` or `ignore` crate).
2. Add file-watcher based hot-reload (optional, behind a feature flag).

---

## Cross-Cutting Architectural Issues

### Async/Blocking Lock Mix
Multiple modules (`session::cache`, `observability::analytics`, `observability::log_analysis`) use `std::sync::RwLock` inside async functions. This is a well-known tokio anti-pattern that can stall the executor.

**Fix:** Audit all `std::sync::RwLock` usages and replace with `tokio::sync::RwLock` where the lock is held across `.await` points.

### Duplicate Content-Length Framing
Both `lsp/` and `mcp/` implement their own `Content-Length` framing readers/writers. There is no shared transport library.

**Fix:** Extract a `jsonrpc_stdio` or `lsp_transport` crate/module shared by both.

### Missing Browser Automation
`src/browser.rs` does not exist. No browser automation capability is present.

**Fix:** Either implement a browser module (e.g., via `fantoccini` / WebDriver) or document the intentional omission.

### Hardcoded Timeouts
Almost every network-facing or subprocess module has hardcoded timeouts (30s, 60s, 500ms). None are configurable via `SelfwareConfig` or environment variables.

**Fix:** Centralize timeout defaults in `crate::config` and allow override via config file or env vars.

### Global Mutable State
- `observability::telemetry::TRACING_GUARD` (OnceLock + Mutex)
- `session::chat_store::EncryptionManager::get()` singleton
- `observability::analytics::ANALYTICS_ENABLED` (AtomicBool)

**Fix:** Inject these as dependencies rather than global singletons to improve testability.

---

## Priority Matrix

| Priority | Module | Issue |
|----------|--------|-------|
| **P0** | `mcp/server.rs` | Container management is pure simulation (no real Docker/Podman) |
| **P0** | `lsp/client.rs` | `did_change` hardcodes version 2 — protocol violation |
| **P0** | `resource/mod.rs` | `monitor_loop` has no cancellation token — task leak |
| **P1** | `session/cache.rs` | Blocking `std::sync::RwLock` in async code |
| **P1** | `self_healing/executor.rs` | `block_in_place` + `thread::sleep` blocks tokio worker |
| **P1** | `devops/process_manager.rs` | Port reservation race window |
| **P2** | `mcp/client.rs` | Only extracts `text` content blocks |
| **P2** | `observability/telemetry.rs` | Silent OpenTelemetry init failures |
| **P2** | `resource/disk.rs` | Hardcoded 10GB model size stub |
| **P3** | `computer/screen.rs` | `capture_region` ignores region |
| **P3** | `computer/window.rs` | macOS stubs return `Ok(())` |
| **P3** | `skills/mod.rs` | No recursive discovery or hot-reload |

---

*End of IMPROVE-09 Services Analysis*
