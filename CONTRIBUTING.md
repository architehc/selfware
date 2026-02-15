# Contributing to Selfware

Thank you for your interest in contributing to Selfware, a Rust-based agentic coding framework for local LLMs. This guide will help you get started.

## Development Setup

### Prerequisites

- **Rust 1.75+** (install via [rustup](https://rustup.rs/))
- Git

### Getting Started

```bash
git clone https://github.com/architehc/selfware.git
cd selfware
cargo build
cargo test
```

## Code Style

All contributions must adhere to the project's code style standards.

- **Formatting:** Run `cargo fmt` before committing. All code must pass `cargo fmt --check`.
- **Linting:** Run `cargo clippy` and resolve all warnings. CI will reject code with clippy warnings.

```bash
cargo fmt
cargo clippy -- -D warnings
```

## Testing

### Unit Tests

```bash
cargo test
```

### Integration Tests

Integration tests are gated behind a feature flag:

```bash
cargo test --features integration
```

All pull requests must pass both unit and integration tests before merging.

## Feature Flags

| Flag          | Description                                      |
|---------------|--------------------------------------------------|
| `integration` | Enables integration tests that require additional runtime dependencies |

## Project Structure

Selfware is organized into several key subdirectories alongside 70+ flat modules at the crate root:

| Directory | Purpose                                      |
|-----------|----------------------------------------------|
| `agent/`  | Multi-agent orchestration and lifecycle       |
| `api/`    | LLM provider interfaces and API abstractions  |
| `tools/`  | 53+ built-in tool implementations             |
| `tui/`    | Terminal UI rendering and garden-themed display|
| `ui/`     | UI component library and layout management    |
| `input/`  | Input handling, key bindings, and event processing |

## Submitting a Pull Request

1. Fork the repository and create a feature branch from `main`.
2. Make your changes, following the code style guidelines above.
3. Add or update tests to cover your changes.
4. Run `cargo fmt`, `cargo clippy`, and `cargo test` locally.
5. Write a clear, descriptive commit message.
6. Open a pull request against `main` with a summary of your changes, the motivation behind them, and any relevant issue numbers.
7. Respond to review feedback promptly.

### PR Checklist

- [ ] Code compiles without warnings (`cargo clippy -- -D warnings`)
- [ ] Code is formatted (`cargo fmt --check`)
- [ ] All tests pass (`cargo test`)
- [ ] Integration tests pass if applicable (`cargo test --features integration`)
- [ ] Documentation is updated if public APIs changed

## Reporting Issues

When opening an issue, please include:

- **Description:** A clear summary of the problem or feature request.
- **Steps to reproduce:** For bugs, provide the minimal steps to trigger the issue.
- **Expected behavior:** What you expected to happen.
- **Actual behavior:** What actually happened, including any error messages or logs.
- **Environment:** Rust version, OS, and any relevant configuration details.

Use the appropriate issue template if one is available. Search existing issues before opening a new one to avoid duplicates.

## License

By contributing to Selfware, you agree that your contributions will be licensed under the same license as the project.
