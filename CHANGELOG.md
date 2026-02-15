# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-02-14

### Added

- Initial release of Selfware, a Rust-based agentic coding framework for local LLMs.
- **53+ built-in tools** for file operations, code analysis, search, shell execution, and more.
- **Multi-agent support** with orchestration and lifecycle management via the `agent/` subsystem.
- **PDVR cognitive cycle** (Perceive, Decide, Validate, Respond) for structured agent reasoning.
- **Checkpoint and journal system** for persisting agent state, enabling recovery and session continuity.
- **Safety layer** with path validation, command filtering, and sandboxed execution to prevent unintended side effects.
- **Garden-themed terminal UI** built on a custom TUI framework with rich visual feedback.
- **Local-first architecture** designed for on-device LLM inference without external API dependencies.
- LLM provider API abstractions via the `api/` module.
- Input handling and key binding system via the `input/` module.
- UI component library and layout management via the `ui/` module.
- Integration test suite behind the `integration` feature flag.

[0.1.0]: https://github.com/architehc/selfware/releases/tag/v0.1.0
