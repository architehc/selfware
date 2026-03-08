# Selfware QA Workflows - Implementation Guide

This guide provides step-by-step instructions for implementing the Selfware QA workflows in your project.

## Table of Contents

1. [Quick Start](#quick-start)
2. [File Structure](#file-structure)
3. [Integration Steps](#integration-steps)
4. [Configuration](#configuration)
5. [CI/CD Setup](#cicd-setup)
6. [Troubleshooting](#troubleshooting)

---

## Quick Start

### For Selfware Maintainers

```bash
# 1. Copy workflow files to your repository
cp -r output/workflows/* .github/workflows/

# 2. Copy scripts
mkdir -p scripts
cp output/scripts/* scripts/
chmod +x scripts/*.py

# 3. Copy configuration
cp output/selfware-qa-schema.yaml .

# 4. Update your main CI to call the orchestrator
# (See CI/CD Setup section below)
```

### For Generated Projects

```bash
# 1. Copy appropriate template
cp -r output/templates/python/* my-generated-project/

# 2. Install dependencies
cd my-generated-project
pip install -e ".[dev]"  # For Python
npm install              # For Node.js
cargo fetch              # For Rust

# 3. Run QA
make qa
```

---

## File Structure

```
output/
├── README.md                          # Main documentation
├── selfware-qa-specification.md       # Full technical spec
├── selfware-qa-schema.yaml            # QA configuration
├── IMPLEMENTATION_GUIDE.md            # This file
├── Makefile                           # Build automation
│
├── workflows/                         # GitHub Actions workflows
│   ├── rust-qa.yml                    # Rust-specific QA
│   ├── python-qa.yml                  # Python-specific QA
│   ├── nodejs-qa.yml                  # Node.js/TS QA
│   └── selfware-qa-orchestrator.yml   # Main orchestrator
│
├── scripts/                           # Orchestration scripts
│   ├── qa-orchestrator.py             # Python orchestrator
│   └── report-aggregator.js           # Report aggregation
│
└── templates/                         # Project templates
    ├── rust/
    │   └── Cargo.toml
    ├── python/
    │   └── pyproject.toml
    └── nodejs/
        ├── package.json
        ├── tsconfig.json
        ├── eslint.config.mjs
        ├── .prettierrc
        └── vitest.config.ts
```

---

## Integration Steps

### Step 1: Copy Workflow Files

```bash
# Create workflows directory
mkdir -p .github/workflows

# Copy all workflow files
cp output/workflows/*.yml .github/workflows/
```

### Step 2: Copy Scripts

```bash
# Create scripts directory
mkdir -p scripts

# Copy orchestrator scripts
cp output/scripts/qa-orchestrator.py scripts/
cp output/scripts/report-aggregator.js scripts/

# Make Python script executable
chmod +x scripts/qa-orchestrator.py

# Install Python dependencies for orchestrator
pip install pyyaml
```

### Step 3: Copy Configuration

```bash
# Copy QA schema
cp output/selfware-qa-schema.yaml .

# Optionally customize for your project
# Edit selfware-qa-schema.yaml to adjust thresholds
```

### Step 4: Update Main CI Workflow

Add to your existing `.github/workflows/ci.yml`:

```yaml
jobs:
  # Your existing jobs...
  
  # Add QA orchestrator
  quality-assurance:
    needs: [build]  # Or your build job
    uses: ./.github/workflows/selfware-qa-orchestrator.yml
    with:
      qa_profile: standard
      working_directory: ./generated
```

### Step 5: Add Makefile (Optional)

```bash
cp output/Makefile .
```

This provides convenient commands like `make qa`, `make test`, `make coverage`.

---

## Configuration

### QA Profiles

Three profiles are available in `selfware-qa-schema.yaml`:

#### Standard (Recommended)
```yaml
qa_profile:
  name: "standard"
  coverage_threshold: 80
  quality_gates:
    - stage: "security"
      severity_threshold: "HIGH"
```

#### Strict (Production)
```yaml
qa_profile:
  name: "strict"
  coverage_threshold: 90
  quality_gates:
    - stage: "security"
      severity_threshold: "MEDIUM"
```

#### Minimal (Prototyping)
```yaml
qa_profile:
  name: "minimal"
  coverage_threshold: 50
  quality_gates:
    - stage: "security"
      severity_threshold: "CRITICAL"
```

### Customizing Thresholds

Edit `selfware-qa-schema.yaml`:

```yaml
qa_profile:
  name: "custom"
  
  coverage:
    min_overall: 85  # Change from 80
    min_per_file: 75
  
  scoring:
    weights:
      test: 0.35     # Increase test weight
      coverage: 0.15 # Decrease coverage weight
```

### Language-Specific Overrides

```yaml
qa_profile:
  language_overrides:
    rust:
      coverage:
        min_overall: 85  # Higher for Rust
    python:
      coverage:
        min_overall: 75  # Lower for Python
```

---

## CI/CD Setup

### GitHub Actions

#### Option 1: Automatic Detection (Recommended)

The orchestrator automatically detects languages:

```yaml
# .github/workflows/main.yml
name: CI

on: [push, pull_request]

jobs:
  qa:
    uses: ./.github/workflows/selfware-qa-orchestrator.yml
    with:
      qa_profile: standard
      working_directory: ./generated
```

#### Option 2: Manual Language Selection

```yaml
jobs:
  rust-qa:
    if: contains(github.event.head_commit.message, '[rust]')
    uses: ./.github/workflows/rust-qa.yml
    with:
      working-directory: ./generated/rust

  python-qa:
    if: contains(github.event.head_commit.message, '[python]')
    uses: ./.github/workflows/python-qa.yml
    with:
      working-directory: ./generated/python
```

#### Option 3: Matrix Strategy

```yaml
jobs:
  qa:
    strategy:
      matrix:
        language: [rust, python, nodejs]
    uses: ./.github/workflows/${{ matrix.language }}-qa.yml
```

### GitLab CI

```yaml
# .gitlab-ci.yml
stages:
  - qa

variables:
  QA_PROFILE: standard

qa:rust:
  stage: qa
  script:
    - cargo check
    - cargo test
  only:
    changes:
      - "**/*.rs"

qa:python:
  stage: qa
  script:
    - pip install -e ".[dev]"
    - pytest
  only:
    changes:
      - "**/*.py"
```

### Azure DevOps

```yaml
# azure-pipelines.yml
trigger:
  branches:
    include:
      - main

stages:
- stage: QA
  jobs:
  - job: RustQA
    condition: contains(variables['Build.SourceVersionMessage'], '[rust]')
    steps:
    - script: cargo test
      displayName: 'Run Rust QA'
```

---

## Troubleshooting

### Common Issues

#### Issue: Coverage below threshold

**Symptoms:**
```
FAIL: Coverage 72% below threshold 80%
```

**Solutions:**
1. Add more tests to uncovered code
2. Exclude non-testable files in config:
   ```yaml
   coverage:
     exclude_patterns:
       - "**/cli.py"
       - "**/main.rs"
   ```
3. Lower threshold (not recommended):
   ```yaml
   coverage_threshold: 75
   ```

#### Issue: Security scan failures

**Symptoms:**
```
CRITICAL: Dependency vulnerability found
```

**Solutions:**
1. Update dependencies:
   ```bash
   cargo update          # Rust
   pip install -U        # Python
   npm audit fix         # Node.js
   ```
2. Review and accept risk (with documentation)
3. Use `severity_threshold: CRITICAL` to only fail on critical issues

#### Issue: Type checking failures

**Symptoms:**
```
error: Function missing return type annotation
```

**Solutions:**
1. Add type annotations
2. For Python, use `typing.Any` temporarily:
   ```python
   from typing import Any
   def func() -> Any: ...
   ```
3. For gradual adoption, disable strict mode:
   ```toml
   [tool.mypy]
   strict = false
   ```

#### Issue: Orchestrator script fails

**Symptoms:**
```
ModuleNotFoundError: No module named 'yaml'
```

**Solutions:**
```bash
pip install pyyaml
```

Or install all dependencies:
```bash
pip install pyyaml pytest pytest-cov
```

### Debug Mode

Enable verbose logging:

```bash
# Python orchestrator
DEBUG=1 python scripts/qa-orchestrator.py --action run --language python

# Node.js aggregator
DEBUG=1 node scripts/report-aggregator.js --input reports/
```

### Getting Help

1. Check the [full specification](./selfware-qa-specification.md)
2. Review [GitHub Issues](https://github.com/architehc/selfware/issues)
3. Run with verbose output: `make qa VERBOSE=1`

---

## Migration Guide

### From Existing CI

If you already have CI setup:

1. **Identify existing jobs** that overlap with QA stages
2. **Replace with reusable workflows** from this package
3. **Add missing stages** (security, coverage, etc.)
4. **Configure quality gates** in `selfware-qa-schema.yaml`

### Example Migration

**Before:**
```yaml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo test
```

**After:**
```yaml
jobs:
  qa:
    uses: ./.github/workflows/rust-qa.yml
    with:
      coverage-threshold: '80'
```

---

## Best Practices

1. **Start with Standard profile** and adjust based on needs
2. **Commit lock files** (Cargo.lock, package-lock.json) for reproducibility
3. **Run QA locally** before pushing: `make qa`
4. **Use pre-commit hooks** to catch issues early
5. **Review unified reports** to track quality trends
6. **Set up notifications** for quality gate failures

---

## Next Steps

1. ✅ Copy workflow files
2. ✅ Copy scripts and configuration
3. ✅ Update CI to use orchestrator
4. ✅ Test with a sample project
5. ✅ Customize thresholds as needed
6. ✅ Train team on new workflow

For questions or issues, refer to the [full specification](./selfware-qa-specification.md).
