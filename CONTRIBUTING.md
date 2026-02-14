# Contributing to Selfware

Thank you for your interest in contributing to Selfware! This document provides guidelines and instructions for contributing.

## Development Setup

### Prerequisites

- Rust 1.75.0 or later (MSRV)
- Git
- A local LLM backend for testing (optional, but recommended)

### Building from Source

```bash
# Clone the repository
git clone https://github.com/architehc/selfware.git
cd selfware

# Build in debug mode
cargo build

# Build with all features
cargo build --all-features

# Build release
cargo build --release
```

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with all features
cargo test --all-features

# Run specific test file
cargo test --test e2e_tools_test

# Run with output
cargo test -- --nocapture
```

### Code Style

We use `rustfmt` for formatting and `clippy` for linting:

```bash
# Format code
cargo fmt

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings
```

## Making Changes

### Branching Strategy

- `main` - stable release branch
- `develop` - integration branch for features
- Feature branches - `feature/your-feature-name`
- Bug fixes - `fix/issue-description`

### Commit Messages

We follow conventional commits:

```
type(scope): description

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks
- `ci`: CI/CD changes

Examples:
```
feat(tools): add file_search tool with regex support
fix(safety): prevent path traversal via symlinks
docs(readme): update installation instructions
```

### Pull Request Process

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes
4. Run tests and linting locally
5. Submit a pull request

PR requirements:
- [ ] All tests pass
- [ ] Clippy passes with no warnings
- [ ] Code is formatted with rustfmt
- [ ] Documentation is updated if needed
- [ ] Commit messages follow convention

## Testing Guidelines

### Unit Tests

Place unit tests in the same file as the code:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        // Test code
    }
}
```

### Integration Tests

Place integration tests in `tests/`:

```rust
// tests/integration/my_test.rs
#[tokio::test]
async fn test_integration_scenario() {
    // Integration test code
}
```

### Test Fixtures

Place test fixtures in `tests/e2e-projects/`.

## Architecture Overview

```
src/
├── agent/          # Core agent logic and execution loop
├── api/            # API client for LLM backends
├── tools/          # Built-in tool implementations
├── tui/            # Terminal UI (feature-gated)
├── ui/             # CLI UI components
├── safety.rs       # Safety validation
├── config.rs       # Configuration management
└── lib.rs          # Library exports
```

### Key Abstractions

- **Agent**: The main execution loop that coordinates LLM calls and tool execution
- **Tool**: Interface for implementing new tools
- **SafetyChecker**: Validates operations before execution
- **Config**: Runtime configuration management

## Adding New Tools

1. Create a new struct implementing the `Tool` trait:

```rust
use async_trait::async_trait;
use serde_json::Value;
use anyhow::Result;

pub struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str {
        "my_tool"
    }

    fn description(&self) -> &str {
        "Description of what this tool does"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "param1": {"type": "string", "description": "Parameter 1"}
            },
            "required": ["param1"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        // Implementation
        Ok(serde_json::json!({"result": "success"}))
    }
}
```

2. Register the tool in `src/tools/mod.rs`

3. Add tests in `tests/unit/test_tools.rs`

## Feature Flags

Available feature flags in `Cargo.toml`:

- `extras` - Enables TUI, caching, and advanced features
- `integration` - Enables integration tests

```bash
# Build with specific features
cargo build --features extras
cargo build --features integration
```

## Questions?

- Open an issue for bugs or feature requests
- Start a discussion for questions or ideas

Thank you for contributing! 🦊
