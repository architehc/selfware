#!/usr/bin/env bash
# Pinned Rule 1 stop-the-line check (AGENTS.md Rule 1)
# Verifies cargo fmt, clippy, and cargo test --lib in an isolated worktree
# at a specific commit SHA with a dedicated CARGO_TARGET_DIR.
# Prevents dirty working-tree contamination and concurrent target/ lock contention.

set -euo pipefail

THIS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${THIS_DIR}/.." && pwd)"

REV="${1:-HEAD}"
SHA="$(cd "${REPO_ROOT}" && git rev-parse --verify "${REV}")"

echo "============================================================"
echo " Rule 1 Stop-the-Line Verification"
echo " Revision: ${REV} (${SHA})"
echo "============================================================"

# Create isolated scratch directory
SCRATCH_DIR="$(mktemp -d "${TMPDIR:-/tmp}/selfware-gate-XXXXXX")"
WORKTREE_DIR="${SCRATCH_DIR}/worktree"
TARGET_DIR="${SCRATCH_DIR}/target"

cleanup() {
    local exit_code=$?
    echo "Cleaning up isolated worktree..."
    if [[ -d "${WORKTREE_DIR}" ]]; then
        (cd "${REPO_ROOT}" && git worktree remove --force "${WORKTREE_DIR}" 2>/dev/null || true)
    fi
    rm -rf "${SCRATCH_DIR}" 2>/dev/null || true
    exit "${exit_code}"
}
trap cleanup EXIT INT TERM

echo "Creating isolated worktree at ${WORKTREE_DIR}..."
(cd "${REPO_ROOT}" && git worktree add --detach "${WORKTREE_DIR}" "${SHA}" -q)

export CARGO_TARGET_DIR="${TARGET_DIR}"
export RUSTFLAGS="-D warnings"

echo "1. cargo fmt --check..."
(cd "${WORKTREE_DIR}" && cargo fmt -- --check)
echo "   cargo fmt OK"

echo "2. cargo clippy --all-targets -- -D warnings..."
(cd "${WORKTREE_DIR}" && cargo clippy --all-targets -- -D warnings)
echo "   cargo clippy OK"

echo "2b. cargo doc --no-deps --features extras (RUSTDOCFLAGS=-D warnings)..."
(cd "${WORKTREE_DIR}" && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features extras)
echo "   cargo doc OK"

echo "3. cargo test --lib </dev/null..."
(cd "${WORKTREE_DIR}" && cargo test --lib </dev/null)
echo "   cargo test --lib OK"

echo ""
echo "============================================================"
echo " PASS: Revision ${SHA} satisfies AGENTS.md Rule 1"
echo "============================================================"
