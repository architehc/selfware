# Selfware development Makefile (Rust-only; QA reports via scripts/)

.PHONY: help qa test test-live coverage bench format lint security report report-md clean

help:
	@echo "Selfware - Available Commands"
	@echo ""
	@echo "  make qa          Run full Rust QA (check, fmt, clippy, test)"
	@echo "  make test        Run the test suite"
	@echo "  make test-live   Run endpoint-dependent integration tests"
	@echo "  make coverage    Generate an HTML coverage report (cargo-llvm-cov)"
	@echo "  make bench       Run benchmarks"
	@echo "  make format      Format all code"
	@echo "  make lint        Run clippy"
	@echo "  make security    Run cargo audit"
	@echo "  make report      Generate unified QA report (scripts/qa-orchestrator.py)"
	@echo "  make report-md   Generate markdown QA report (scripts/report-aggregator.js)"
	@echo "  make clean       Clean generated files"

qa:
	cargo check --all-targets --all-features
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test --features extras

test:
	cargo test --features extras

test-live:
	cargo test --features integration --test integration

coverage:
	cargo llvm-cov --features extras --html

bench:
	cargo bench

format:
	cargo fmt --all

lint:
	cargo clippy --all-targets -- -D warnings
	cargo clippy --all-targets --all-features -- -D warnings

security:
	cargo audit

report:
	python scripts/qa-orchestrator.py \
		--action aggregate \
		--config selfware-qa-schema.yaml \
		--reports-dir reports/ \
		--output reports/unified-report.json

report-md:
	node scripts/report-aggregator.js \
		--report reports/unified-report.json \
		--format markdown \
		--output reports/qa-report.md

clean:
	rm -rf reports/
	cargo clean
