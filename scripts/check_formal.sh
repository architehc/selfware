#!/usr/bin/env bash
# Re-check the formal lifecycle models and their exported transition tables.
#
#   0. No model may contain a proof escape hatch: `sorry`, `admit`,
#      `axiom`, `native_decide` or `unsafe` fail the check (a comment
#      mentioning one is fine — only code is scanned).
#   1. Every formal/*.lean model must elaborate with no errors (every
#      theorem is re-proved).
#   2. The tables the models print (`#eval exportTable`) are decoded into
#      JSON and compared with the committed formal/*_table.json, which the
#      Rust conformance tests check the code against:
#        TaskFsm.lean              -> task_table.json (src/lifecycle)
#        ResourceFsm.lean          -> resource_table.json (src/lifecycle)
#        HarnessLoopBounds.lean    -> agent_state_table.json
#                                     (src/agent/loop_control.rs)
#        VerificationGateBounds.lean -> verification_gate_table.json
#                                     (Agent::check_completion_gate)
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

# Proof escape hatches, in code only: strip `--` line comments and `/- … -/`
# block comments first.
escapes="$(python3 - "${FORMAL}" <<'PY'
import pathlib, re, sys
bad = []
for path in sorted(pathlib.Path(sys.argv[1]).glob("*.lean")):
    code = re.sub(r"/-.*?-/", "", path.read_text(), flags=re.S)
    code = re.sub(r"--[^\n]*", "", code)
    for word in ("sorry", "admit", "axiom", "native_decide", "unsafe"):
        if re.search(r"\b" + word + r"\b", code):
            bad.append(f"{path.name}: {word}")
print("\n".join(bad))
PY
)"
if [ -n "${escapes}" ]; then
    echo "check_formal: FAILED — proof escape hatch(es) in formal/:" >&2
    echo "${escapes}" >&2
    exit 1
fi
echo "check_formal: no sorry/admit/axiom/native_decide/unsafe in formal/*.lean"

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
        echo "check_formal: if the model changed on purpose, rerun with --write and update the Rust side to match." >&2
        exit 1
    fi
    rm -f "${generated}"
    echo "check_formal: ${table#"${REPO_ROOT}/"} matches ($(grep -c '^  \[' "${table}") transitions)."
}

check_model TaskFsm.lean "${FORMAL}/task_table.json"
check_model ResourceFsm.lean "${FORMAL}/resource_table.json"
check_model HarnessLoopBounds.lean "${FORMAL}/agent_state_table.json"
check_model VerificationGateBounds.lean "${FORMAL}/verification_gate_table.json"
if [ "${WRITE}" -eq 0 ]; then
    echo "check_formal: OK — all models check and every exported table matches."
fi
