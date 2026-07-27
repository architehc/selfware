# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.5] - 2026-07-27

### Added
- Apply isolation: shadow-worktree staging, bounded diff, one-use merge token, compile gate, two-step UI
- Multi-language context tiers: Python/JS/TS/Go are Code-layer nodes (envelope ships them verbatim; Lite measured==shipped)
- Embedded UI assets (index/app/style/editor + d3/lucide) — the released binary serves a working Evolve UI with no source checkout
- Evidence trust gate, review protocol (typed 422s, one repair, trust_state), custom component checklist, symbol-level retrieval

### Fixed (review rounds 6-8)
- Credential hygiene: MCP resources AND tools redact secrets; cargo/git/workflow spawns sanitize env; `[models.*]` profile endpoints gated; SELFWARE_CONFIG only for trusted configs
- Honest status: merge recomputes digest (bytes bound to preview); MCP isError never success-cached; failed workflows exit non-zero; full_verify non-vacuous; macOS stubs error honestly
- Data integrity: `/undo` restores tip checkpoint; custom selection survives refresh; panic payloads hidden; `tool_parser` + parse + output-cap char-boundary fixes
- Apply self-blocks removed (`.selfware/` + `.worktrees/` exempt); graph excludes `.worktrees/`; dead-code excludes test fns; readiness falls back to plain cargo test; graceful shutdown exits 0
- Safety pattern false-positive tuning (rm globs, eval substitutions, checksum pipes, python -c, quoted strings, env prefixes, read-only absolute reads, chown -R, mkfs)
- Selected-document review excludes inline tests in Full mode; envelope Custom ships hand-picked docs; trust gate markup classification (docs report-only)
- no-default-features build green; MSRV 1.95; coverage floor 60%+ratchet; CI system-tests clippy

## [0.6.4] - 2026-07-26

### Added
- **Apply isolation**: `/api/actions/apply` now stages agent runs in isolated `evolve-apply-*` shadow worktrees pinned to HEAD, serialized under a global lock — the live checkout is never touched during a run
- Bounded diff verification: scope limited to `src/`+`docs/` (rejections name the path), empty diffs honestly rejected, sha256 digest binds the exact patch
- One-use merge: `POST /api/actions/apply/commit` merges only on exact digest match + unchanged HEAD (`409 base_moved`); atomic consumption; ff-only merge with safe checkout first
- Two-step apply flow in the UI (staged diff preview → Apply)

### Fixed
- Evolution daemon: `cargo fmt` runs before compile/test (committed bytes == tested bytes)
- Honest status: `full_verify` no longer passes vacuously; MCP `isError` never cached as success; failed workflows exit non-zero; macOS computer-control stubs error honestly
- Patch deletions (`+++ /dev/null`) validate the old path and are checkpointed
- Legacy `codegraph` bin deprecated; `evidence_complete` documented as file-coverage
- Keyring failures visible without `RUST_LOG`; scanner lock-poisoning reported via `scan_warnings`; audit log flushes per event
- Linux build breakers (mouse `button`, keyboard `warn!` import)

## [0.6.3] - 2026-07-26

### Added
- Symbol-level context retrieval (`/api/context/select/symbols`, per-symbol `expand`)
- Evidence trust gate: `422 context_trust_blocked` for high-severity non-trusted content; hidden-unicode detection incl. TAG chars; `gate-context-trust` preset
- `selfware run --preset <id>`; AGENTS.md working agreements; stop-the-line pre-commit gate (fmt + clippy `-D warnings`)
- DNS-rebinding Host-header guard on the evolve server; disk-maintenance hourly wiring
- Skeleton: multi-line signature capture, scoped `pub(in …)` visibility, `const fn` classification; envelope ships non-Rust files verbatim
- Custom context checklist UI (per-component lite/full tokens, Apply/Clear, filter, ResizeObserver graph)

### Fixed (four external review rounds — safety, integrity, honesty)
- **Credential hygiene**: cargo/git spawns sanitize env (no more `SELFWARE_API_KEY` to build.rs/hooks); `shell_exec` env-map injection block; untrusted-remote-endpoint load refusal; `protected_branches` reset; keyring read-back verification
- **Evolution daemon**: `tests/` protected; test-count-regression winners rejected; `generations ≥ 1`; dry-run key redaction
- **Honest status**: `full_verify` no longer vacuous; MCP `isError` never cached as success; failed workflows exit non-zero; usage accumulated across repair; `evidence_complete=false` on unreadable files; macOS computer stubs error honestly
- **Data integrity**: `/undo` restores the tip checkpoint (first edit undoable); custom selection survives refresh; panic payloads no longer leak to clients; `tool_parser` multi-byte panic; lessons sanitized before prompt injection
- **Validation**: `rm --no-preserve-root` matched; `sudo`/env-prefix can't bypass protected-branch guard; `export`/`env` injection wrappers covered; `$IFS` normalization; patch deletions validated via old path
- **Agent context**: `compress_to_fit` contract (double-subtraction) + skeleton-less eviction; memory index leak on consolidate; RSI circuit breaker counts non-improving cycles; chars/4 estimators removed

### Changed
- `llmfit-core` moved to crates.io 1.1 (crate is publishable); keyring 3.6; config warnings no longer double-print

## [0.6.2] - 2026-07-26

### Added
- Symbol-level context retrieval: `GET /api/context/select/symbols` (task → relevant symbols, not files, via the dependency graph) and per-symbol `expand` (`/api/context/expand?component=..&symbol=..`) returning exactly one function/struct/enum span — the smallest precise retrieval unit for tiny windows
- `config set-key` read-back verification: keychain writes that silently fail to persist (observed on non-GUI macOS sessions) now report an honest error with the config-file/env fallback instead of claiming success

### Changed
- Config warnings no longer print twice when logging is enabled (stderr fallback only when no tracing subscriber)
- keyring crate bumped to 3.6

## [0.6.1] - 2026-07-26

### Added
- Evidence trust gate: assistant sends scan evidence through `evolve::context_trust` before any model call — high-severity findings in non-trusted content block with typed `422 context_trust_blocked`; trusted first-party code is reported, never blocked (`trust_gate` summary on every review response)
- `selfware run --preset <id>`: renders an evolve preset's task + invariants into a headless run; unknown ids list the library
- `gate-context-trust` preset (safety direction restored to the library)
- `AGENTS.md` working agreements: stop-the-line, review-gate sign-off for subtractions, honest status, measured-not-estimated

### Changed
- crates.io-publishable by default: `llmfit-core` moved from pinned git rev to crates.io 1.1 (`unpack` hardware calibration unchanged)
- Stop-the-line pre-commit gate (fmt + clippy `-D warnings`); all clippy warnings cleared
- `codegraph_viewer.html`: `--danger` variable, d3 link id unwrapping, bare-path root categorization; component checklist preserves scroll while filtering

## [0.6.0] - 2026-07-25

### Added
- Auto context tiers: `context_mode = "auto"` (default) + `context_fit_ratio` measure each tier and pick the richest that fits the model window — validated live from 8k to 1M windows
- ContextEnvelope: evidence paths ship tier-projected content (Map=cards, Lite=skeletons, Compact=reduced source) bound by a shared `content_hash`; pinned over-budget tiers rejected with typed `422 context_over_budget`
- Custom context mode: per-component hand-picked selection (`POST /api/context/custom`) with a filterable checklist UI showing per-component lite/full token breakdown
- Review protocol: typed outcomes (`model_output_invalid`/`empty`/`ungrounded` 422s with retained model/latency/usage telemetry), one budgeted repair retry, and computed `trust_state` (`structural`/`degraded`/`verified`) end-to-end into the UI status
- Auto-tier picker UI with measured budget bar; deep-linkable inspector tabs (`#inspector=context`)
- `HttpEmbeddingProvider` bearer auth + `[models.embedding]` profile support (verified with OpenRouter `qwen/qwen3-embedding-8b`)
- Benchmarks: `examples/tier_bench.rs` (tier perf/accuracy), `scripts/model_matrix_bench.sh` (14-model declared+measured capability matrix incl. vision probes), `scripts/context_quality_bench.sh` (retrieval-quality per tier)
- `ComponentCard.lite_tokens` per-component skeleton cost

### Changed
- Agent `ContextLevel` unified onto the shared `evolve::ContextMode` vocabulary; skeleton extraction shared in `evolve::skeleton`
- `endpoint_smoke` probe: larger token budget + reasoning-chunk visibility for thinking models
- Context reduction: comment/inline-test stripping and duplicate function-body elision across assembled context

### Removed
- Dead `safety::context_guard` heuristic scanner (620 lines, zero production callers) — see `docs/CONSOLIDATION_PLAN.md` §8

## [0.3.1-beta.1] - 2026-07-17

### Added
- Git commit SHA embedded in version output via `build.rs` (`selfware --version` now prints e.g. `0.3.1-beta.1+g7e0c3e3c`; plain version outside a git checkout)
- First-run auth: `OPENROUTER_API_KEY` environment fallback, 401 remediation guidance, and `config set-key`
- Init/config wizards validate before writing and probe the endpoint

### Changed
- Version bumped to `0.3.1-beta.1` — `0.3.0-beta.1` semver-sorted below the already-published v0.3.0
- Config warnings are now visible and the loaded config path is printed (provenance)
- Help output reorganized
- Docs truth pass across README and docs/
- Conservative default `context_length` for unknown models

### Fixed
- Panic in the `status` command
- Global flags now accepted in trailing position
- Headless mode fails fast in Normal (ask) mode instead of blocking on prompts
- Completion-gate honesty, including an explicit blocked-by-safety outcome
- MCP newline-delimited framing
- Min-steps ordering

### Removed
- Large dead-code cleanup (commit 7e0c3e3c)

## [0.3.0-beta.1] - 2026-04-12

### Added
- SWL lowering now generates warnings for guardrail enforcement steps
- Guardrails attached to delegate steps are now properly lowered
- `as_str()` method added to `sandbox::RiskLevel` enum
- 16 concurrent stream support for Qwen3.5-122B-A10B-NVFP4-yarn-1010k (1M context)
- Full codebase audit completed with 6 specialized agents
- Endpoint verification and configuration fixes
- Stub documentation - all placeholder modules now clearly marked
- Code coverage improvements (targeting 90%)
- Configuration cleanup - obsolete configs moved to configs/obsolete/
- TUI dashboard mode with real-time telemetry display
- Event infrastructure for agent-TUI communication (`TuiEvent`, `SharedDashboardState`)
- Dependabot configuration for automated dependency updates
- Codecov integration for coverage tracking
- Release notes categorization template
- Docker support with multi-stage build (Dockerfile, .dockerignore)
- Examples directory with 4 usage examples (basic_chat, run_task, multi_agent, custom_config)
- 53 new tests for agent module (state transitions, tool handling, error recovery)
- 24 new tests for API client (retry logic, request construction, response parsing)
- Self-healing recovery system with `ErrorClass` classification (Network, Timeout, RateLimit, ResourceExhaustion, ParseError, AuthError, Unknown)
- Exponential backoff with jitter (base * 2^attempt +/-25%, capped at 30s) for retry actions
- Automatic escalation chains: primary strategy fails -> escalation strategy runs
- Per-pattern retry state tracking to prevent infinite recovery loops
- `reset_retry()` for clearing backoff state after successful operations
- Recovery executor with real `thread::sleep` delays, checkpoint restore, cache clearing
- Custom recovery actions: `compress_context`, `reduce_tool_set`, `switch_parsing_mode`
- E2E system test harness (`system_tests/projecte2e/`) with 7 scenarios across 3 difficulty levels
- ANSI terminal capture via `script` for E2E test screenshots
- Scored E2E reports with error analysis and Markdown output
- `LlmClient` trait abstraction for testable API interactions
- `Drop` implementation for `ProcessManager` to clean up child processes
- Configurable API request timeout (default 120s)
- Swarm access log capped at 10,000 entries via `VecDeque`
- `spawn_blocking` wrapper for file search operations

### Changed
- CI workflow now runs tests with `--all-features`
- Switched to `taiki-e/install-action` for faster cargo tool installation
- Added caching to release workflow builds
- Coverage job now runs on all branches (not just main)
- Recovery counter now increments before attempt (prevents infinite failed recovery loops)
- `handle_error()` in `SelfHealingEngine` now classifies errors and selects class-specific strategies
- Agent loop resets self-healing retry state after each successful step

### Fixed
- Missing `PathBuf` import in CLI module when `bench-harness` feature enabled
- Missing `warn` imports in `batch`, `browser` modules
- Clippy error: never_loop in `swl/guardrails/engine.rs`
- Test failure: `swebench::tests::test_load_tasks` mock data mismatch
- Test failure: `swl::lowering::tests::lower_code_review_produces_executor_workflow` missing warnings
- Various clippy warnings (unused variables, boolean simplification, clamp patterns)
- Repository URLs in Cargo.toml and README.md now point to correct location
- Recovery counter bug: `recovery_attempts` was only incremented on success, allowing infinite failed attempts
- `uuid_v4()` in self-healing now uses `uuid::Uuid::new_v4()` instead of timestamp-based fake
- Unicode width calculation in agent avatar for multi-byte characters
- `partial_cmp` unwrap replaced with `unwrap_or(Ordering::Equal)` in swarm sorting
- `from_utf8_lossy` unnecessary allocations in output processing
- Substring path matching in contract testing replaced with proper path checks
- `ArrayContaining` matcher logic corrected for subset validation

### Security
- Updated CI to include security audit job
- **Critical**: Hardened shell command validation with regex-based matching and obfuscation detection
- **Critical**: Fixed path traversal bypass via canonical path validation only
- Added symlink chain validation to prevent symlink-based attacks
- Added detection for base64-encoded command execution
- Added command chaining detection (`;`, `&&`, `||`)
- Added netcat reverse shell pattern detection
- Added eval with command substitution detection

## [0.1.0] - 2026-02-13

### Added
- Initial release
- Core agent framework with PDVR cognitive cycle
- 53 built-in tools for file operations, git, cargo, search, and more
- Safety system with path validation and command filtering
- Checkpoint system for task persistence
- Multi-agent collaboration support
- TUI mode with ratatui (feature-gated)
- Garden visualization for codebase health
- Support for multiple LLM backends (vLLM, Ollama, llama.cpp, LM Studio)
- YOLO mode for autonomous operation
- Workflow DSL for complex task automation

### Security
- Path traversal protection
- Dangerous command blocking
- Protected paths system
- Git force push prevention

[Unreleased]: https://github.com/architehc/selfware/compare/v0.6.5...HEAD
[0.6.5]: https://github.com/architehc/selfware/compare/v0.6.4...v0.6.5
[0.6.4]: https://github.com/architehc/selfware/compare/v0.6.3...v0.6.4
[0.6.3]: https://github.com/architehc/selfware/compare/v0.6.2...v0.6.3
[0.6.2]: https://github.com/architehc/selfware/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/architehc/selfware/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/architehc/selfware/compare/v0.3.1-beta.1...v0.6.0
[0.3.1-beta.1]: https://github.com/architehc/selfware/compare/v0.3.0...v0.3.1-beta.1
[0.1.0]: https://github.com/architehc/selfware/releases/tag/v0.1.0
