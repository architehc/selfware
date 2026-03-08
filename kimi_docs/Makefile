# Selfware QA Workflows Makefile
# Provides convenient commands for running QA tasks

.PHONY: help install qa qa-rust qa-python qa-nodejs coverage bench clean

# Default target
help:
	@echo "Selfware QA Workflows - Available Commands"
	@echo ""
	@echo "Setup:"
	@echo "  make install          Install all dependencies"
	@echo ""
	@echo "Quality Assurance:"
	@echo "  make qa               Run full QA pipeline"
	@echo "  make qa-rust          Run Rust QA only"
	@echo "  make qa-python        Run Python QA only"
	@echo "  make qa-nodejs        Run Node.js QA only"
	@echo ""
	@echo "Testing:"
	@echo "  make test             Run all tests"
	@echo "  make coverage         Generate coverage reports"
	@echo "  make bench            Run benchmarks"
	@echo ""
	@echo "Maintenance:"
	@echo "  make clean            Clean generated files"
	@echo "  make format           Format all code"
	@echo "  make lint             Run all linters"
	@echo "  make security         Run security scans"
	@echo ""
	@echo "Reports:"
	@echo "  make report           Generate unified QA report"
	@echo "  make report-md        Generate markdown report"

# Installation
install:
	@echo "Installing dependencies..."
	pip install -r requirements.txt 2>/dev/null || true
	npm install 2>/dev/null || true
	cargo fetch 2>/dev/null || true

# Full QA Pipeline
qa: qa-rust qa-python qa-nodejs
	@echo "✅ Full QA pipeline complete"

qa-rust:
	@echo "🔧 Running Rust QA..."
	@if [ -f Cargo.toml ]; then \
		cargo check --all-features && \
		cargo fmt --all -- --check && \
		cargo clippy --all-features -- -D warnings && \
		cargo test --all-features && \
		echo "✅ Rust QA passed"; \
	else \
		echo "⏭️  No Rust code found"; \
	fi

qa-python:
	@echo "🐍 Running Python QA..."
	@if [ -f pyproject.toml ] || [ -f setup.py ]; then \
		ruff check src/ tests/ && \
		ruff format --check src/ tests/ && \
		mypy src/ tests/ && \
		pytest --cov=src --cov-report=term-missing && \
		echo "✅ Python QA passed"; \
	else \
		echo "⏭️  No Python code found"; \
	fi

qa-nodejs:
	@echo "📦 Running Node.js QA..."
	@if [ -f package.json ]; then \
		npm run lint && \
		npm run format:check && \
		npm run typecheck && \
		npm run test && \
		echo "✅ Node.js QA passed"; \
	else \
		echo "⏭️  No Node.js code found"; \
	fi

# Testing
test: test-rust test-python test-nodejs
	@echo "✅ All tests passed"

test-rust:
	@echo "🔧 Running Rust tests..."
	@cargo test --all-features

test-python:
	@echo "🐍 Running Python tests..."
	@pytest -v

test-nodejs:
	@echo "📦 Running Node.js tests..."
	@npm test

# Coverage
coverage: coverage-rust coverage-python coverage-nodejs
	@echo "✅ Coverage reports generated"

coverage-rust:
	@echo "🔧 Generating Rust coverage..."
	@cargo tarpaulin --out Html --out Xml

coverage-python:
	@echo "🐍 Generating Python coverage..."
	@pytest --cov=src --cov-report=html --cov-report=xml

coverage-nodejs:
	@echo "📦 Generating Node.js coverage..."
	@npm run test:coverage

# Benchmarks
bench: bench-rust bench-python bench-nodejs
	@echo "✅ Benchmarks complete"

bench-rust:
	@echo "🔧 Running Rust benchmarks..."
	@cargo bench

bench-python:
	@echo "🐍 Running Python benchmarks..."
	@pytest --benchmark-only

bench-nodejs:
	@echo "📦 Running Node.js benchmarks..."
	@npm run test:bench

# Formatting
format: format-rust format-python format-nodejs
	@echo "✅ Code formatted"

format-rust:
	@echo "🔧 Formatting Rust code..."
	@cargo fmt --all

format-python:
	@echo "🐍 Formatting Python code..."
	@ruff format src/ tests/

format-nodejs:
	@echo "📦 Formatting Node.js code..."
	@npm run format

# Linting
lint: lint-rust lint-python lint-nodejs
	@echo "✅ Linting complete"

lint-rust:
	@echo "🔧 Linting Rust code..."
	@cargo clippy --all-features -- -D warnings

lint-python:
	@echo "🐍 Linting Python code..."
	@ruff check src/ tests/

lint-nodejs:
	@echo "📦 Linting Node.js code..."
	@npm run lint

# Security
security: security-rust security-python security-nodejs
	@echo "✅ Security scans complete"

security-rust:
	@echo "🔧 Scanning Rust dependencies..."
	@cargo audit

security-python:
	@echo "🐍 Scanning Python dependencies..."
	@bandit -r src/ || true
	@safety check || true

security-nodejs:
	@echo "📦 Scanning Node.js dependencies..."
	@npm audit --audit-level=moderate

# Reports
report:
	@echo "📊 Generating unified QA report..."
	@python scripts/qa-orchestrator.py \
		--action aggregate \
		--config selfware-qa-schema.yaml \
		--reports-dir reports/ \
		--output reports/unified-report.json

report-md:
	@echo "📄 Generating markdown report..."
	@node scripts/report-aggregator.js \
		--report reports/unified-report.json \
		--format markdown \
		--output reports/qa-report.md

# Cleaning
clean:
	@echo "🧹 Cleaning generated files..."
	@rm -rf target/ 2>/dev/null || true
	@rm -rf dist/ 2>/dev/null || true
	@rm -rf build/ 2>/dev/null || true
	@rm -rf __pycache__/ 2>/dev/null || true
	@rm -rf .pytest_cache/ 2>/dev/null || true
	@rm -rf .mypy_cache/ 2>/dev/null || true
	@rm -rf coverage/ 2>/dev/null || true
	@rm -rf htmlcov/ 2>/dev/null || true
	@rm -rf *.egg-info/ 2>/dev/null || true
	@rm -rf node_modules/ 2>/dev/null || true
	@rm -f .coverage 2>/dev/null || true
	@rm -f cobertura.xml 2>/dev/null || true
	@rm -f coverage.xml 2>/dev/null || true
	@cargo clean 2>/dev/null || true
	@echo "✅ Clean complete"

# Development helpers
dev-rust:
	@echo "🔧 Starting Rust development..."
	@cargo watch -x check -x test

dev-python:
	@echo "🐍 Starting Python development..."
	@ptw --runner pytest

dev-nodejs:
	@echo "📦 Starting Node.js development..."
	@npm run dev

# CI helpers (for GitHub Actions)
ci-rust: format-rust lint-rust test-rust coverage-rust
	@echo "✅ Rust CI checks complete"

ci-python: format-python lint-python test-python coverage-python
	@echo "✅ Python CI checks complete"

ci-nodejs: format-nodejs lint-nodejs test-nodejs coverage-nodejs
	@echo "✅ Node.js CI checks complete"
