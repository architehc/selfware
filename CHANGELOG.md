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

### Changed
- CI workflow now runs tests with `--all-features`
- Switched to `taiki-e/install-action` for faster cargo tool installation
- Added caching to release workflow builds
- Coverage job now runs on all branches (not just main)

### Fixed
- Repository URLs in Cargo.toml and README.md now point to correct location
- Removed no-op CLI flags (`--compact`, `--verbose`, `--show-tokens`)

### Security
- Updated CI to include security audit job

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
