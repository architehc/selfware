# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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

[Unreleased]: https://github.com/architehc/selfware/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/architehc/selfware/releases/tag/v0.1.0
