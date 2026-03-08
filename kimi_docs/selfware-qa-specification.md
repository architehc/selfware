# Selfware Agentic Harness - Comprehensive Quality Assurance Workflows
## Technical Specification v1.0

---

## Executive Summary

This document provides a comprehensive quality assurance framework for the Selfware agentic harness, supporting multi-language code generation (Rust, Python, Node.js, TypeScript) with progressive testing, unified validation, and automated quality gates.

---

## 1. LANGUAGE-SPECIFIC QA TOOLCHAINS

### 1.1 RUST

#### Testing Frameworks and Tools

| Tool | Purpose | Recommendation |
|------|---------|----------------|
| **Built-in Test** (`cargo test`) | Unit and integration testing | PRIMARY - Zero config, native support |
| **Tarpaulin** | Code coverage | PRIMARY - 80%+ threshold configured |
| **Criterion.rs** | Benchmark testing | PRIMARY - Statistical benchmarking |
| **Proptest** | Property-based testing | RECOMMENDED - Fuzz-like testing |
| **Mockall** | Mocking framework | RECOMMENDED - For unit isolation |
| **Fake** | Test data generation | RECOMMENDED - Fixtures and factories |

**Configuration Example (Cargo.toml dev-dependencies):**
```toml
[dev-dependencies]
tokio-test = "0.4"
mockall = "0.13"
proptest = "1.6"
criterion = { version = "0.5", features = ["html_reports"] }
fake = { version = "4.0", features = ["derive"] }
insta = { version = "1.42", features = ["yaml", "redactions"] }
```

#### Linting and Code Quality

| Tool | Purpose | Priority |
|------|---------|----------|
| **Clippy** | Linting | REQUIRED - `cargo clippy -- -D warnings` |
| **rustfmt** | Formatting | REQUIRED - Enforced in CI |
| **cargo-deny** | License/audit check | REQUIRED - Dependency validation |
| **cargo-machete** | Unused deps | RECOMMENDED - Keep deps lean |
| **cargo-udeps** | Unused deps | ALTERNATIVE - CI integration |

**Clippy Configuration (.clippy.toml):**
```toml
avoid-breaking-exported-api = false
disallowed-names = ["foo", "bar", "baz"]
enum-variant-name-threshold = 3
```

#### Type Checking
- **Native**: Rust's compiler provides exhaustive type checking
- **Additional**: `cargo check` for fast feedback, `cargo rustc -- -D warnings` for strict mode

#### CI/CD Pipeline Pattern (Rust)

```yaml
# .github/workflows/rust-qa.yml
name: Rust Quality Assurance

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  check:
    name: Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --all-features

  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --all-features -- -D warnings

  test:
    name: Test
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        rust: [stable, beta]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust }}
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --all-features --verbose

  coverage:
    name: Code Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo install cargo-tarpaulin
      - run: cargo tarpaulin --out Xml --fail-under 80
      - uses: codecov/codecov-action@v4
        with:
          files: ./cobertura.xml

  audit:
    name: Security Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: rustsec/audit-check@v2
        with:
          token: ${{ secrets.GITHUB_TOKEN }}

  bench:
    name: Benchmark
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo bench -- --test
```

#### Package Management
- **Primary**: Cargo (native)
- **Lock file**: `Cargo.lock` committed for binaries
- **Update strategy**: Dependabot + `cargo update` in CI

#### Security Scanning
- **cargo-audit**: Vulnerability scanning via RustSec
- **cargo-deny**: License compliance, banned crates
- **Trivy**: Container image scanning

#### Performance Testing
- **Criterion.rs**: Statistical benchmarking with regression detection
- **cargo-flamegraph**: CPU profiling
- **cargo-heaptrack**: Memory profiling
- **iai-callgrind**: Instruction-level benchmarking

---

### 1.2 PYTHON

#### Testing Frameworks and Tools

| Tool | Purpose | Recommendation |
|------|---------|----------------|
| **pytest** | Primary test runner | REQUIRED - Industry standard |
| **pytest-cov** | Coverage | REQUIRED - 80%+ threshold |
| **pytest-xdist** | Parallel testing | RECOMMENDED - Speed up CI |
| **pytest-asyncio** | Async testing | REQUIRED - For async code |
| **hypothesis** | Property-based testing | RECOMMENDED - Fuzz-like |
| **factory-boy** | Test fixtures | RECOMMENDED - Data generation |
| **responses** | HTTP mocking | RECOMMENDED - API testing |
| **freezegun** | Time mocking | RECOMMENDED - Date/time tests |

**Configuration Example (pyproject.toml):**
```toml
[project.optional-dependencies]
dev = [
    "pytest>=8.0",
    "pytest-cov>=5.0",
    "pytest-asyncio>=0.23",
    "pytest-xdist>=3.5",
    "hypothesis>=6.100",
    "factory-boy>=3.3",
    "responses>=0.25",
    "freezegun>=1.5",
    "moto>=5.0",  # AWS mocking
]
```

#### Linting and Code Quality

| Tool | Purpose | Priority |
|------|---------|----------|
| **Ruff** | Fast linter + formatter | REQUIRED - Replaces flake8/black |
| **mypy** | Static type checking | REQUIRED - Strict mode |
| **bandit** | Security linting | REQUIRED - SAST |
| **pydocstyle** | Docstring conventions | RECOMMENDED |

**Ruff Configuration (pyproject.toml):**
```toml
[tool.ruff]
target-version = "py311"
line-length = 100
select = [
    "E",   # pycodestyle errors
    "F",   # Pyflakes
    "I",   # isort
    "N",   # pep8-naming
    "W",   # pycodestyle warnings
    "UP",  # pyupgrade
    "B",   # flake8-bugbear
    "C4",  # flake8-comprehensions
    "SIM", # flake8-simplify
    "ARG", # flake8-unused-arguments
]
ignore = ["E501"]  # Line length handled by formatter

[tool.ruff.format]
quote-style = "double"
indent-style = "space"
```

**MyPy Configuration:**
```toml
[tool.mypy]
python_version = "3.11"
strict = true
warn_return_any = true
warn_unused_configs = true
disallow_untyped_defs = true
disallow_incomplete_defs = true
check_untyped_defs = true
```

#### Type Checking
- **mypy**: Primary static type checker
- **pyright**: Alternative from Microsoft (Pylance backend)
- **beartype**: Runtime type checking (for critical paths)

#### CI/CD Pipeline Pattern (Python)

```yaml
# .github/workflows/python-qa.yml
name: Python Quality Assurance

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  lint:
    name: Lint & Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.11"
      - uses: astral-sh/ruff-action@v1
        with:
          src: "./src"
      - uses: astral-sh/ruff-action@v1
        with:
          args: "format --check"

  typecheck:
    name: Type Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.11"
      - run: pip install mypy
      - run: mypy src/ tests/

  test:
    name: Test (Python ${{ matrix.python }})
    runs-on: ubuntu-latest
    strategy:
      matrix:
        python: ["3.10", "3.11", "3.12"]
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: ${{ matrix.python }}
      - run: pip install -e ".[dev]"
      - run: pytest --cov=src --cov-report=xml --cov-fail-under=80
      - uses: codecov/codecov-action@v4

  security:
    name: Security Scan
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.11"
      - run: pip install bandit safety
      - run: bandit -r src/
      - run: safety check
```

#### Package Management
- **Primary**: `pip` + `pyproject.toml` (PEP 621)
- **Lock files**: `pip-tools` or `uv pip compile`
- **Alternative**: Poetry or PDM for complex projects
- **Virtual env**: `venv` or `uv venv`

#### Security Scanning
- **bandit**: SAST for Python
- **safety**: Dependency vulnerability scanning
- **pip-audit**: Alternative to safety
- **Trivy**: Container scanning

#### Performance Testing
- **pytest-benchmark**: Benchmark testing
- **py-spy**: Sampling profiler
- **memray**: Memory profiler
- **scalene**: CPU+memory profiler

---

### 1.3 NODE.JS / TYPESCRIPT

#### Testing Frameworks and Tools

| Tool | Purpose | Recommendation |
|------|---------|----------------|
| **Vitest** | Primary test runner | REQUIRED - Fast, modern, ESM-native |
| **@vitest/coverage-v8** | Coverage | REQUIRED - Built-in |
| **Playwright** | E2E testing | REQUIRED - Cross-browser |
| **MSW** | API mocking | RECOMMENDED - Service worker mocking |
| **faker-js** | Test data | RECOMMENDED - Data generation |
| **fast-check** | Property testing | RECOMMENDED - Fuzz-like |

**Configuration Example (package.json):**
```json
{
  "devDependencies": {
    "vitest": "^2.0",
    "@vitest/coverage-v8": "^2.0",
    "@playwright/test": "^1.45",
    "msw": "^2.3",
    "@faker-js/faker": "^8.4",
    "fast-check": "^3.19",
    "@testing-library/jest-dom": "^6.4"
  }
}
```

#### Linting and Code Quality

| Tool | Purpose | Priority |
|------|---------|----------|
| **ESLint** | Linting | REQUIRED - With TypeScript plugin |
| **Prettier** | Formatting | REQUIRED - Opinionated formatter |
| **typescript-eslint** | TS-specific rules | REQUIRED |
| **eslint-plugin-security** | Security rules | REQUIRED |
| **eslint-plugin-import** | Import validation | RECOMMENDED |

**ESLint Configuration (eslint.config.mjs):**
```javascript
import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import security from "eslint-plugin-security";

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.strictTypeChecked,
  ...tseslint.configs.stylisticTypeChecked,
  security.configs.recommended,
  {
    languageOptions: {
      parserOptions: {
        project: "./tsconfig.json",
      },
    },
    rules: {
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/explicit-function-return-type": "warn",
      "security/detect-object-injection": "error",
    },
  }
);
```

**Prettier Configuration (.prettierrc):**
```json
{
  "semi": true,
  "trailingComma": "es5",
  "singleQuote": false,
  "printWidth": 100,
  "tabWidth": 2
}
```

#### Type Checking
- **TypeScript Compiler**: `tsc --noEmit` for type-only checking
- **Strict mode**: Enabled for all new code
- **skipLibCheck**: true for faster builds

#### CI/CD Pipeline Pattern (Node.js/TypeScript)

```yaml
# .github/workflows/nodejs-qa.yml
name: Node.js Quality Assurance

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  lint:
    name: Lint & Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: "20"
          cache: "npm"
      - run: npm ci
      - run: npm run lint
      - run: npm run format:check

  typecheck:
    name: Type Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: "20"
          cache: "npm"
      - run: npm ci
      - run: npm run typecheck

  test:
    name: Test (Node ${{ matrix.node }})
    runs-on: ubuntu-latest
    strategy:
      matrix:
        node: ["18", "20", "22"]
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: ${{ matrix.node }}
          cache: "npm"
      - run: npm ci
      - run: npm run test:coverage
      - uses: codecov/codecov-action@v4

  e2e:
    name: E2E Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: "20"
          cache: "npm"
      - run: npm ci
      - run: npx playwright install --with-deps
      - run: npm run test:e2e

  security:
    name: Security Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: "20"
          cache: "npm"
      - run: npm audit --audit-level=moderate
      - run: npx better-npm-audit audit
```

#### Package Management
- **Primary**: npm (v10+) or pnpm (faster)
- **Lock file**: `package-lock.json` or `pnpm-lock.yaml`
- **Update strategy**: Dependabot + `npm audit fix`

#### Security Scanning
- **npm audit**: Built-in vulnerability scanning
- **Snyk**: Comprehensive SCA
- **Trivy**: Container scanning
- **eslint-plugin-security**: Code-level security

#### Performance Testing
- **Vitest bench**: Built-in benchmarking
- **k6**: Load testing
- **autocannon**: HTTP benchmarking
- **clinic.js**: Performance profiling

---

## 2. CROSS-LANGUAGE INTEGRATION

### 2.1 Project Structure

```
selfware/
├── .github/
│   └── workflows/
│       ├── ci.yml                    # Main CI orchestrator
│       ├── rust-qa.yml               # Rust-specific QA
│       ├── python-qa.yml             # Python-specific QA
│       ├── nodejs-qa.yml             # Node.js/TS QA
│       └── security.yml              # Cross-language security
├── harness/
│   ├── core/                         # Rust core (existing)
│   ├── templates/
│   │   ├── rust/                     # Rust project templates
│   │   ├── python/                   # Python project templates
│   │   ├── nodejs/                   # Node.js project templates
│   │   └── typescript/               # TypeScript project templates
│   └── validators/                   # Cross-language validators
├── generated/
│   └── .gitkeep                      # Generated code output
├── reports/
│   └── unified/                      # Unified QA reports
└── scripts/
    ├── qa-orchestrator.py            # Main QA orchestrator
    └── report-aggregator.js          # Report aggregation
```

### 2.2 Unified Validation Patterns

#### Configuration Schema (YAML)

```yaml
# selfware-qa-schema.yaml
qa_profile:
  name: "standard"
  description: "Standard QA profile for agentic code generation"
  
  stages:
    - name: "syntax"
      description: "Syntax validation"
      required: true
      tools:
        rust: ["cargo check"]
        python: ["python -m py_compile"]
        nodejs: ["tsc --noEmit"]
        typescript: ["tsc --noEmit"]
    
    - name: "lint"
      description: "Code quality checks"
      required: true
      tools:
        rust: ["cargo clippy"]
        python: ["ruff check"]
        nodejs: ["eslint"]
        typescript: ["eslint"]
    
    - name: "format"
      description: "Format validation"
      required: true
      tools:
        rust: ["cargo fmt --check"]
        python: ["ruff format --check"]
        nodejs: ["prettier --check"]
        typescript: ["prettier --check"]
    
    - name: "test"
      description: "Unit and integration tests"
      required: true
      coverage_threshold: 80
      tools:
        rust: ["cargo test"]
        python: ["pytest --cov"]
        nodejs: ["vitest run --coverage"]
        typescript: ["vitest run --coverage"]
    
    - name: "security"
      description: "Security scanning"
      required: true
      tools:
        rust: ["cargo audit"]
        python: ["bandit", "safety"]
        nodejs: ["npm audit"]
        typescript: ["npm audit"]
    
    - name: "performance"
      description: "Performance benchmarks"
      required: false
      tools:
        rust: ["cargo bench"]
        python: ["pytest-benchmark"]
        nodejs: ["vitest bench"]
        typescript: ["vitest bench"]

  quality_gates:
    - stage: "syntax"
      fail_on_error: true
    - stage: "lint"
      fail_on_error: true
    - stage: "format"
      fail_on_error: true
    - stage: "test"
      fail_on_error: true
      min_coverage: 80
    - stage: "security"
      fail_on_error: true
      severity_threshold: "HIGH"
```

### 2.3 Unified Reporting

#### Report Format (JSON)

```json
{
  "report_version": "1.0",
  "timestamp": "2026-03-09T12:00:00Z",
  "project": "generated-service",
  "languages": ["rust", "python"],
  "summary": {
    "total_files": 42,
    "total_lines": 3500,
    "passed": 38,
    "failed": 2,
    "skipped": 2
  },
  "stages": [
    {
      "name": "syntax",
      "status": "passed",
      "duration_ms": 1250,
      "results": {
        "rust": { "status": "passed", "files_checked": 15 },
        "python": { "status": "passed", "files_checked": 27 }
      }
    },
    {
      "name": "test",
      "status": "passed",
      "duration_ms": 8500,
      "coverage": {
        "overall": 87.5,
        "rust": 89.2,
        "python": 85.8
      },
      "results": {
        "rust": {
          "status": "passed",
          "tests_run": 45,
          "tests_passed": 45,
          "tests_failed": 0
        },
        "python": {
          "status": "passed",
          "tests_run": 62,
          "tests_passed": 62,
          "tests_failed": 0
        }
      }
    }
  ],
  "quality_score": 92
}
```

### 2.4 Containerization Strategy

#### Multi-Language Dockerfile

```dockerfile
# Dockerfile.multi-lang
FROM rust:1.91-slim-bookworm AS rust-builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM python:3.11-slim-bookworm AS python-builder
WORKDIR /app
COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt
COPY src ./src

FROM node:20-bookworm-slim AS node-builder
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build

# Final runtime image
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    python3.11 \
    python3-pip \
    nodejs \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /app/target/release/selfware /usr/local/bin/
COPY --from=python-builder /app /opt/python-app
COPY --from=node-builder /app/dist /opt/node-app

WORKDIR /workspace
CMD ["selfware"]
```

---

## 3. QUALITY GATES FOR AGENTIC CODE GENERATION

### 3.1 Automated Code Review Patterns

#### Review Checklist (Automated)

```yaml
# review-checklist.yaml
code_review:
  structure:
    - check: "module_organization"
      description: "Modules follow language conventions"
      languages: ["rust", "python", "nodejs", "typescript"]
    
    - check: "naming_conventions"
      description: "Consistent naming (snake_case, camelCase, PascalCase)"
      languages: ["rust", "python", "nodejs", "typescript"]
    
    - check: "documentation"
      description: "Public APIs have docstrings/comments"
      threshold: 80  # 80% of public APIs
      languages: ["rust", "python", "nodejs", "typescript"]
  
  correctness:
    - check: "error_handling"
      description: "Proper error handling patterns"
      languages: ["rust", "python", "nodejs", "typescript"]
    
    - check: "resource_management"
      description: "Resources properly managed (RAII, context managers)"
      languages: ["rust", "python", "nodejs", "typescript"]
    
    - check: "async_patterns"
      description: "Async/await used correctly"
      languages: ["rust", "python", "nodejs", "typescript"]
  
  security:
    - check: "input_validation"
      description: "All inputs validated/sanitized"
      severity: "CRITICAL"
    
    - check: "no_secrets"
      description: "No hardcoded secrets"
      tools: ["gitleaks", "trufflehog"]
    
    - check: "dependency_vulns"
      description: "No known vulnerabilities in dependencies"
      severity: "HIGH"
```

### 3.2 Test Coverage Requirements

| Language | Minimum Coverage | Target Coverage |
|----------|------------------|-----------------|
| Rust | 80% | 90% |
| Python | 80% | 90% |
| Node.js | 80% | 90% |
| TypeScript | 80% | 90% |

**Coverage Exclusions:**
- Auto-generated code
- CLI entry points
- Debug/logging code
- UI rendering (if applicable)

### 3.3 Documentation Standards

| Language | Standard | Tool |
|----------|----------|------|
| Rust | rustdoc | Built-in |
| Python | Google/NumPy style | mkdocs + mkdocstrings |
| Node.js | JSDoc | TypeDoc |
| TypeScript | TSDoc | TypeDoc |

**Documentation Coverage:**
- 100% of public APIs must have documentation
- Examples required for complex functions
- README with usage examples

### 3.4 Security Compliance Checks

```yaml
security_gates:
  sast:
    - tool: "semgrep"
      rules: ["p/owasp-top-ten", "p/cwe-top-25"]
    - tool: "codeql"
      languages: ["rust", "python", "javascript"]
  
  sca:
    - tool: "dependency-check"
      fail_on_cvss: 7.0
  
  secrets:
    - tool: "gitleaks"
      scan_depth: 100
  
  compliance:
    - standard: "OWASP ASVS"
      level: 2
```

---

## 4. WORKFLOW PIPELINE DESIGN

### 4.1 Pipeline Stages

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    SELFWARE AGENTIC QA PIPELINE                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐          │
│  │ GENERATE │───▶│  VALIDATE│───▶│   TEST   │───▶│  REPORT  │          │
│  └──────────┘    └──────────┘    └──────────┘    └──────────┘          │
│       │               │               │               │                 │
│       ▼               ▼               ▼               ▼                 │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐          │
│  │• Parse   │    │• Syntax  │    │• Unit    │    │• Unified │          │
│  │  request│    │• Lint    │    │• Integr. │    │  report  │          │
│  │• Select │    │• Format  │    │• Property│    │• Score   │          │
│  │  template│   │• TypeCk  │    │• E2E     │    │• Feedback│          │
│  │• Generate│    │          │    │          │    │          │          │
│  └──────────┘    └──────────┘    └──────────┘    └──────────┘          │
│       │               │               │               │                 │
│       ▼               ▼               ▼               ▼                 │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐          │
│  │ Checkpoint│   │ Security │    │ Coverage │    │ Decision │          │
│  │  (save)  │    │• Audit   │    │• Bench   │    │• Pass?   │          │
│  │          │    │• SAST    │    │• Fuzz    │    │• Retry?  │          │
│  │          │    │• Secrets │    │          │    │• Escalate│          │
│  └──────────┘    └──────────┘    └──────────┘    └──────────┘          │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 4.2 Stage Definitions

#### Stage 1: Generate
- Parse natural language request
- Select appropriate template
- Generate initial code
- Create checkpoint

#### Stage 2: Validate
- Syntax validation
- Linting
- Format checking
- Type checking
- Security scan (fast)

#### Stage 3: Test
- Unit tests (generated + existing)
- Integration tests
- Property-based tests
- Coverage verification
- Benchmarks (if applicable)

#### Stage 4: Security (Deep)
- Full SAST scan
- Dependency audit
- Secret detection
- Compliance check

#### Stage 5: Report
- Aggregate all results
- Calculate quality score
- Generate unified report
- Provide feedback

### 4.3 Feedback Loops

```yaml
feedback_loops:
  auto_fix:
    enabled: true
    max_iterations: 3
    triggers:
      - lint_errors
      - format_issues
      - type_errors
    
  retry_with_context:
    enabled: true
    max_iterations: 2
    triggers:
      - test_failures
      - coverage_below_threshold
    context_injection:
      - error_messages
      - stack_traces
      - coverage_report
  
  escalation:
    enabled: true
    triggers:
      - security_vulnerabilities
      - max_iterations_exceeded
      - quality_score_below: 70
    actions:
      - notify_human
      - create_issue
      - halt_pipeline
```

### 4.4 Error Handling and Recovery

```python
# qa_orchestrator.py - Error handling pattern
class QAOrchestrator:
    async def run_pipeline(self, request: GenerationRequest) -> PipelineResult:
        checkpoint = None
        iteration = 0
        max_iterations = 3
        
        while iteration < max_iterations:
            try:
                # Stage 1: Generate
                code = await self.generate(request)
                checkpoint = await self.save_checkpoint(code)
                
                # Stage 2: Validate
                validation = await self.validate(code)
                if not validation.passed:
                    code = await self.auto_fix(code, validation.errors)
                    continue
                
                # Stage 3: Test
                test_result = await self.test(code)
                if not test_result.passed:
                    if iteration < max_iterations - 1:
                        request.add_context(test_result.failures)
                        iteration += 1
                        continue
                    else:
                        raise TestFailureException(test_result)
                
                # Stage 4: Security
                security = await self.security_scan(code)
                if security.critical_vulns > 0:
                    raise SecurityException(security)
                
                # Stage 5: Report
                report = await self.generate_report(code, validation, test_result, security)
                
                return PipelineResult(
                    success=True,
                    code=code,
                    report=report,
                    checkpoint=checkpoint
                )
                
            except RecoverableError as e:
                logger.warning(f"Recoverable error: {e}")
                iteration += 1
                if iteration >= max_iterations:
                    raise MaxIterationsExceeded(e)
                    
            except NonRecoverableError as e:
                logger.error(f"Non-recoverable error: {e}")
                await self.escalate(e, checkpoint)
                raise
        
        raise MaxIterationsExceeded("Pipeline exceeded maximum iterations")
```

---

## 5. IMPLEMENTATION RECOMMENDATIONS

### 5.1 Tool Integration Matrix

| Function | Rust | Python | Node.js | TypeScript |
|----------|------|--------|---------|------------|
| Test Runner | cargo test | pytest | vitest | vitest |
| Coverage | tarpaulin | pytest-cov | @vitest/coverage-v8 | @vitest/coverage-v8 |
| Linter | clippy | ruff | eslint | eslint |
| Formatter | rustfmt | ruff | prettier | prettier |
| Type Check | rustc | mypy | tsc | tsc |
| Security | cargo-audit | bandit | npm audit | npm audit |
| Benchmark | criterion | pytest-benchmark | vitest bench | vitest bench |
| Mocking | mockall | pytest-mock | vitest | vitest |

### 5.2 CI/CD Integration

```yaml
# .github/workflows/selfware-qa-orchestrator.yml
name: Selfware QA Orchestrator

on:
  workflow_dispatch:
    inputs:
      generation_request:
        description: 'Generation request JSON'
        required: true
  push:
    paths:
      - 'generated/**'

jobs:
  detect-languages:
    runs-on: ubuntu-latest
    outputs:
      languages: ${{ steps.detect.outputs.languages }}
    steps:
      - uses: actions/checkout@v4
      - id: detect
        run: |
          LANGUAGES="[]"
          [ -f "generated/Cargo.toml" ] && LANGUAGES=$(echo $LANGUAGES | jq '. + ["rust"]')
          [ -f "generated/pyproject.toml" ] && LANGUAGES=$(echo $LANGUAGES | jq '. + ["python"]')
          [ -f "generated/package.json" ] && LANGUAGES=$(echo $LANGUAGES | jq '. + ["nodejs"]')
          echo "languages=$LANGUAGES" >> $GITHUB_OUTPUT

  orchestrate-qa:
    needs: detect-languages
    runs-on: ubuntu-latest
    strategy:
      matrix:
        language: ${{ fromJson(needs.detect-languages.outputs.languages) }}
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup ${{ matrix.language }}
        uses: ./.github/actions/setup-${{ matrix.language }}
      
      - name: Run QA Pipeline
        run: |
          python scripts/qa-orchestrator.py \
            --language ${{ matrix.language }} \
            --config selfware-qa-schema.yaml \
            --output reports/${{ matrix.language }}-report.json
      
      - name: Upload Report
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.language }}-report
          path: reports/${{ matrix.language }}-report.json

  aggregate-reports:
    needs: orchestrate-qa
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          path: reports/
          pattern: "*-report"
      
      - name: Aggregate Reports
        run: |
          node scripts/report-aggregator.js \
            --input reports/ \
            --output reports/unified-report.json
      
      - name: Quality Gate
        run: |
          SCORE=$(jq '.quality_score' reports/unified-report.json)
          if [ "$SCORE" -lt 80 ]; then
            echo "Quality score $SCORE below threshold (80)"
            exit 1
          fi
```

### 5.3 Configuration Files

#### Pre-commit Hooks (.pre-commit-config.yaml)

```yaml
repos:
  # Rust
  - repo: local
    hooks:
      - id: rust-fmt
        name: Rust Format
        entry: cargo fmt -- --check
        language: system
        files: \\.rs$
        pass_filenames: false
      
      - id: rust-clippy
        name: Rust Clippy
        entry: cargo clippy -- -D warnings
        language: system
        files: \\.rs$
        pass_filenames: false
  
  # Python
  - repo: https://github.com/astral-sh/ruff-pre-commit
    rev: v0.6.9
    hooks:
      - id: ruff
        args: [--fix]
      - id: ruff-format
  
  # Node.js/TypeScript
  - repo: local
    hooks:
      - id: eslint
        name: ESLint
        entry: npx eslint --fix
        language: node
        files: \\.(ts|tsx|js|jsx)$
      
      - id: prettier
        name: Prettier
        entry: npx prettier --write
        language: node
        files: \\.(ts|tsx|js|jsx|json|yaml|yml|md)$
```

---

## 6. QUALITY SCORING ALGORITHM

```python
class QualityScorer:
    """Calculate overall quality score for generated code."""
    
    WEIGHTS = {
        "syntax": 0.10,
        "lint": 0.15,
        "format": 0.05,
        "test": 0.30,
        "coverage": 0.20,
        "security": 0.15,
        "performance": 0.05,
    }
    
    def calculate(self, results: dict) -> dict:
        scores = {}
        
        # Syntax score (binary)
        scores["syntax"] = 100 if results["syntax"]["passed"] else 0
        
        # Lint score (error count based)
        lint_errors = results["lint"].get("error_count", 0)
        scores["lint"] = max(0, 100 - lint_errors * 5)
        
        # Format score (binary)
        scores["format"] = 100 if results["format"]["passed"] else 0
        
        # Test score (pass rate)
        test = results["test"]
        if test["total"] > 0:
            scores["test"] = (test["passed"] / test["total"]) * 100
        else:
            scores["test"] = 0
        
        # Coverage score (min threshold 80%)
        coverage = results["coverage"]["overall"]
        scores["coverage"] = min(100, (coverage / 80) * 100) if coverage < 80 else 100
        
        # Security score (vulnerability based)
        security = results["security"]
        critical = security.get("critical", 0)
        high = security.get("high", 0)
        medium = security.get("medium", 0)
        scores["security"] = max(0, 100 - critical * 50 - high * 20 - medium * 5)
        
        # Performance score (benchmark comparison)
        perf = results.get("performance", {})
        scores["performance"] = perf.get("score", 100)
        
        # Weighted total
        total = sum(scores[k] * self.WEIGHTS[k] for k in self.WEIGHTS)
        
        return {
            "overall": round(total, 1),
            "breakdown": scores,
            "grade": self._grade(total),
            "passed": total >= 80 and scores["security"] >= 70
        }
    
    def _grade(self, score: float) -> str:
        if score >= 95: return "S"
        if score >= 90: return "A"
        if score >= 80: return "B"
        if score >= 70: return "C"
        if score >= 60: return "D"
        return "F"
```

---

## 7. SUMMARY

This specification provides a comprehensive QA framework for the Selfware agentic harness:

1. **Language-specific toolchains** for Rust, Python, Node.js, and TypeScript
2. **Unified validation patterns** with configurable quality gates
3. **Cross-language integration** via orchestrator and shared reporting
4. **Progressive testing** from syntax to security
5. **Feedback loops** for iterative improvement
6. **Quality scoring** for objective assessment

### Key Files to Create:
- `.github/workflows/rust-qa.yml`
- `.github/workflows/python-qa.yml`
- `.github/workflows/nodejs-qa.yml`
- `.github/workflows/selfware-qa-orchestrator.yml`
- `selfware-qa-schema.yaml`
- `scripts/qa-orchestrator.py`
- `scripts/report-aggregator.js`
- `pyproject.toml` templates
- `package.json` templates

### Quality Thresholds:
- Minimum coverage: 80%
- Minimum quality score: 80/100
- Security: No critical vulnerabilities
- Grade target: B or higher
