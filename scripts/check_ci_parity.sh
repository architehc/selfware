#!/usr/bin/env bash
# Local CI-parity check for the GitHub jobs the Rule 1 gate does not cover
# (AGENTS.md Rule 1: recommended before every push).
#
# Runs, in an isolated worktree at a pinned SHA with its own CARGO_TARGET_DIR:
#   1. Lint job, "Test release and evaluation integrity" step: the python
#      scripts/tests suite in a bare venv (no numpy/playwright, like the
#      ubuntu runner) plus the `bash -n` syntax checks.
#   2. Documentation job: `cargo doc --no-deps --features extras` with
#      RUSTDOCFLAGS=-D warnings (catches private/broken intra-doc links).
#   3. Test (no default features) job: `cargo test --no-default-features`
#      (catches tests/items that forget their feature cfg).
#
# Git runs with init.defaultBranch=master, the stock default on CI runners
# (a local Apple/Homebrew git may default to `main` and hide branch-name
# assumptions in test fixtures).
#
# Usage: scripts/check_ci_parity.sh [REV]
#   SW_PARITY_TARGET_DIR=<dir>  reuse a target dir across runs (faster)

set -euo pipefail

THIS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${THIS_DIR}/.." && pwd)"

REV="${1:-HEAD}"
SHA="$(cd "${REPO_ROOT}" && git rev-parse --verify "${REV}")"

echo "============================================================"
echo " CI-parity check (python suite, docs, no-default-features)"
echo " Revision: ${REV} (${SHA})"
echo "============================================================"

SCRATCH_DIR="$(mktemp -d "${TMPDIR:-/tmp}/selfware-ci-parity-XXXXXX")"
WORKTREE_DIR="${SCRATCH_DIR}/worktree"
VENV_DIR="${SCRATCH_DIR}/venv"

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

export CARGO_TARGET_DIR="${SW_PARITY_TARGET_DIR:-${SCRATCH_DIR}/target}"
export CARGO_TERM_COLOR=never
# Emulate the CI runner's git default branch for every git the tests spawn.
export GIT_CONFIG_COUNT=1
export GIT_CONFIG_KEY_0=init.defaultBranch
export GIT_CONFIG_VALUE_0=master

echo "1. python scripts/tests suite (bare venv, no optional deps)..."
python3 -m venv "${VENV_DIR}"
(cd "${WORKTREE_DIR}" \
    && "${VENV_DIR}/bin/python" -m unittest discover -s scripts/tests \
    && bash -n scripts/redteam_wave_pipeline.sh \
    && bash -n benchmarks/harbor/harness-search.sh)
echo "   python suite OK"

echo "2. cargo doc --no-deps --features extras (RUSTDOCFLAGS=-D warnings)..."
(cd "${WORKTREE_DIR}" && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features extras)
echo "   cargo doc OK"

echo "3. cargo test --no-default-features </dev/null..."
(cd "${WORKTREE_DIR}" && cargo test --no-default-features </dev/null)
echo "   cargo test --no-default-features OK"

echo ""
echo "============================================================"
echo " PASS: Revision ${SHA} passes the CI-parity checks"
echo "============================================================"
