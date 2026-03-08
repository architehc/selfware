# Selfware Quality Assurance Workflows

Comprehensive quality assurance framework for the Selfware agentic harness, supporting multi-language code generation with progressive testing, unified validation, and automated quality gates.

## Overview

This repository contains the complete QA infrastructure for Selfware, an autonomous AI workshop that generates code in multiple programming languages. The QA system ensures:

- **Code Quality**: Linting, formatting, and type checking across all languages
- **Test Coverage**: Minimum 80% coverage requirement with automated reporting
- **Security**: SAST, SCA, and secret detection
- **Performance**: Benchmarking and profiling capabilities
- **Unified Reporting**: Cross-language quality scoring and feedback

## Supported Languages

| Language | Test Runner | Linter | Formatter | Type Checker | Coverage |
|----------|-------------|--------|-----------|--------------|----------|
| **Rust** | cargo test | Clippy | rustfmt | rustc | Tarpaulin |
| **Python** | pytest | Ruff | Ruff | mypy | pytest-cov |
| **Node.js** | Vitest | ESLint | Prettier | tsc | v8 |
| **TypeScript** | Vitest | ESLint | Prettier | tsc | v8 |

## Quick Start

### 1. Setup QA for a New Project

```bash
# Copy the appropriate template
cp -r templates/python/* my-project/
cd my-project

# Install dependencies
pip install -e ".[dev]"

# Run QA locally
python scripts/qa-orchestrator.py --action run --language python --config selfware-qa-schema.yaml
```

### 2. Run QA Pipeline

```bash
# Run full QA pipeline
python scripts/qa-orchestrator.py \
  --action run \
  --language rust \
  --config selfware-qa-schema.yaml \
  --working-dir ./generated
```

### 3. Aggregate Reports

```bash
# Aggregate multiple language reports
python scripts/qa-orchestrator.py \
  --action aggregate \
  --config selfware-qa-schema.yaml \
  --languages '["rust", "python"]' \
  --reports-dir reports/ \
  --output unified-report.json
```

## QA Pipeline Stages

```
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│ GENERATE │───▶│ VALIDATE │───▶│   TEST   │───▶│  REPORT  │
└──────────┘    └──────────┘    └──────────┘    └──────────┘
     │               │               │               │
     ▼               ▼               ▼               ▼
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│• Parse   │    │• Syntax  │    │• Unit    │    │• Unified │
│  request │    │• Lint    │    │• Integr. │    │  report  │
│• Select  │    │• Format  │    │• Property│    │• Score   │
│  template│    │• TypeCk  │    │• E2E     │    │• Feedback│
│• Generate│    │          │    │          │    │          │
└──────────┘    └──────────┘    └──────────┘    └──────────┘
```

### Stage Details

1. **Syntax**: Ensure code compiles/parses successfully
2. **Format**: Validate consistent code style
3. **Lint**: Static analysis for code quality
4. **Type Check**: Validate type correctness
5. **Test**: Run unit, integration, and property-based tests
6. **Security**: Scan for vulnerabilities and secrets
7. **Performance**: Run benchmarks (optional)

## Quality Gates

| Gate | Requirement | Fail Action |
|------|-------------|-------------|
| Syntax | Must compile | Stop pipeline |
| Format | Must follow style | Stop pipeline |
| Lint | No errors | Stop pipeline |
| Type Check | All types valid | Stop pipeline |
| Test | 100% pass, 80% coverage | Stop pipeline |
| Security | No HIGH/CRITICAL issues | Stop pipeline |

## Quality Scoring

The quality score is calculated using weighted categories:

| Category | Weight | Description |
|----------|--------|-------------|
| Syntax | 10% | Code compiles/parses |
| Format | 5% | Code follows style guide |
| Lint | 15% | No linting errors |
| Type Check | 10% | All types valid |
| Test | 30% | Test pass rate |
| Coverage | 20% | Code coverage % |
| Security | 10% | No vulnerabilities |

### Grade Scale

| Grade | Score | Description |
|-------|-------|-------------|
| S | 95-100 | Exceptional |
| A | 90-94 | Excellent |
| B | 80-89 | Good (minimum pass) |
| C | 70-79 | Acceptable |
| D | 60-69 | Below standard |
| F | <60 | Failed |

## Configuration

### QA Profiles

Three built-in profiles are available:

#### Standard (default)
- 80% minimum coverage
- All required stages
- Balanced for general use

#### Strict
- 90% minimum coverage
- All stages required
- No HIGH security issues allowed
- For production-critical code

#### Minimal
- 50% minimum coverage
- Syntax and test only
- CRITICAL security issues only
- For rapid prototyping

### Custom Configuration

```yaml
# selfware-qa-schema.yaml
qa_profile:
  name: "custom"
  stages:
    - name: "syntax"
      required: true
      tools:
        rust: ["cargo check"]
        python: ["python -m py_compile"]
  
  quality_gates:
    - stage: "test"
      min_coverage: 85
```

## CI/CD Integration

### GitHub Actions

```yaml
# .github/workflows/selfware-qa.yml
name: Selfware QA

on: [push, pull_request]

jobs:
  qa:
    uses: ./.github/workflows/selfware-qa-orchestrator.yml
    with:
      qa_profile: standard
      working_directory: ./generated
```

### Local Development

```bash
# Run all checks
make qa

# Run specific language
make qa-rust
make qa-python
make qa-nodejs

# Generate coverage report
make coverage

# Run benchmarks
make bench
```

## Directory Structure

```
.
├── .github/
│   └── workflows/
│       ├── rust-qa.yml              # Rust-specific QA
│       ├── python-qa.yml            # Python-specific QA
│       ├── nodejs-qa.yml            # Node.js/TS QA
│       └── selfware-qa-orchestrator.yml  # Main orchestrator
├── scripts/
│   ├── qa-orchestrator.py           # Python orchestrator
│   └── report-aggregator.js         # Report aggregation
├── templates/
│   ├── rust/
│   │   └── Cargo.toml               # Rust project template
│   ├── python/
│   │   └── pyproject.toml           # Python project template
│   └── nodejs/
│       ├── package.json             # Node.js project template
│       ├── tsconfig.json            # TypeScript config
│       ├── eslint.config.mjs        # ESLint config
│       ├── .prettierrc              # Prettier config
│       └── vitest.config.ts         # Vitest config
├── selfware-qa-specification.md     # Full specification
└── selfware-qa-schema.yaml          # QA configuration schema
```

## Tools Reference

### Rust

| Tool | Purpose | Install |
|------|---------|---------|
| cargo test | Testing | Built-in |
| cargo clippy | Linting | Built-in |
| cargo fmt | Formatting | Built-in |
| cargo tarpaulin | Coverage | `cargo install cargo-tarpaulin` |
| cargo audit | Security | `cargo install cargo-audit` |
| cargo bench | Benchmarking | Built-in |

### Python

| Tool | Purpose | Install |
|------|---------|---------|
| pytest | Testing | `pip install pytest` |
| ruff | Lint/Format | `pip install ruff` |
| mypy | Type checking | `pip install mypy` |
| bandit | Security | `pip install bandit` |
| safety | Dependencies | `pip install safety` |

### Node.js/TypeScript

| Tool | Purpose | Install |
|------|---------|---------|
| vitest | Testing | `npm install vitest` |
| eslint | Linting | `npm install eslint` |
| prettier | Formatting | `npm install prettier` |
| tsc | Type checking | `npm install typescript` |
| playwright | E2E testing | `npm install @playwright/test` |

## Feedback Loops

The QA system supports three feedback mechanisms:

### 1. Auto-Fix
Automatically fixes fixable issues (formatting, some lint errors)
- Max iterations: 3
- Triggers: lint errors, format issues, type errors

### 2. Retry with Context
Retries generation with error context
- Max iterations: 2
- Triggers: test failures, low coverage
- Context: error messages, stack traces, coverage report

### 3. Escalation
Escalates to human when automated fixes fail
- Triggers: security vulnerabilities, max iterations exceeded, score < 70
- Actions: notify human, create issue, halt pipeline

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run the QA pipeline: `make qa`
5. Submit a pull request

## License

MIT License - see LICENSE file for details

## Resources

- [Selfware Website](https://selfware.design)
- [Selfware GitHub](https://github.com/architehc/selfware)
- [Full Specification](./selfware-qa-specification.md)
