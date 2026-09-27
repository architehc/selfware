#!/usr/bin/env bash
# Re-check the formal lifecycle models and their exported transition tables.
#
#   1. `lean formal/WorkflowBounds.lean`, `formal/TaskFsm.lean` and
#      `formal/ResourceFsm.lean` must elaborate with no errors (every theorem
#      is re-proved).
#   2. The tables TaskFsm.lean and ResourceFsm.lean print (`#eval
#      exportTable`) are decoded into JSON and compared with the committed
#      formal/task_table.json and formal/resource_table.json, which the Rust
#      conformance tests (`lifecycle` tests) check src/lifecycle against.
#
# Usage: scripts/check_formal.sh [--write]
#   --write   regenerate the committed tables instead of comparing.
#
# Needs Lean 4 (core only, no Mathlib) and python3. When `lean` is not on
# PATH the check is skipped with exit 0 and a clear message: the Rust
# conformance test still pins the committed table. With
# CHECK_FORMAL_REQUIRE_LEAN=1 (the CI `formal` job) a missing `lean` is a
# failure instead, so the job can never go green without checking anything.

set -euo pipefail

THIS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${THIS_DIR}/.." && pwd)"
FORMAL="${REPO_ROOT}/formal"

WRITE=0
case "${1:-}" in
    --write) WRITE=1 ;;
    "") ;;
    *) echo "usage: $0 [--write]" >&2; exit 2 ;;
esac

if ! command -v lean >/dev/null 2>&1; then
    if [ "${CHECK_FORMAL_REQUIRE_LEAN:-0}" = "1" ]; then
        echo "check_formal: FAILED — 'lean' is not on PATH and CHECK_FORMAL_REQUIRE_LEAN=1." >&2
        exit 1
    fi
    echo "check_formal: SKIPPED — 'lean' is not on PATH (install Lean 4, e.g. via elan)."
    echo "check_formal: the committed formal/*_table.json files are still enforced by the Rust conformance test."
    exit 0
fi
if ! command -v python3 >/dev/null 2>&1; then
    echo "check_formal: python3 is required to decode the exported table." >&2
    exit 1
fi

echo "check_formal: $(lean --version)"

echo "check_formal: lean formal/WorkflowBounds.lean"
lean "${FORMAL}/WorkflowBounds.lean"

# Elaborate one model that ends in `#eval exportTable`, decode the printed
# table into JSON (one [state, event, next] row per line) and compare it with
# (or, with --write, write it to) the committed table.
check_model() {
    local model="$1" table="$2"
    echo "check_formal: lean formal/${model}"
    local raw generated
    raw="$(lean "${FORMAL}/${model}")"
    generated="$(mktemp)"
    # The #eval prints a Lean string literal (JSON-compatible escaping):
    # decode it to the JSON array it contains.
    printf '%s\n' "${raw}" | tail -n 1 | python3 -c '
import json, sys
rows = json.loads(json.loads(sys.stdin.read()))
assert all(len(r) == 3 for r in rows), "every row is [state, event, next]"
print("[\n" + ",\n".join("  " + json.dumps(r) for r in rows) + "\n]")
' > "${generated}"

    if [ "${WRITE}" -eq 1 ]; then
        cp "${generated}" "${table}"
        rm -f "${generated}"
        echo "check_formal: wrote ${table} ($(grep -c '^  \[' "${table}") transitions)"
        return 0
    fi
    if ! diff -u "${table}" "${generated}"; then
        rm -f "${generated}"
        echo "check_formal: FAILED — ${table#"${REPO_ROOT}/"} differs from ${model}'s export." >&2
        echo "check_formal: if the model changed on purpose, rerun with --write and update src/lifecycle to match." >&2
        exit 1
    fi
    rm -f "${generated}"
    echo "check_formal: ${table#"${REPO_ROOT}/"} matches ($(grep -c '^  \[' "${table}") transitions)."
}

check_model TaskFsm.lean "${FORMAL}/task_table.json"
check_model ResourceFsm.lean "${FORMAL}/resource_table.json"
if [ "${WRITE}" -eq 0 ]; then
    echo "check_formal: OK — all models check and both exported tables match."
fi
