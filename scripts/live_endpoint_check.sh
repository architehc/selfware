#!/usr/bin/env bash
# Live check against a real OpenAI-compatible endpoint (default:
# llm.selfware.design). Runs a short read-only review task headless and checks
# the structured outcome, not the model's prose:
#   - the endpoint answers /models (an unreachable endpoint FAILS; it is never
#     reported as a skip — AGENTS.md rule 3)
#   - the run ends with exactly one JSON result object whose exit_status
#     equals the process exit code
#   - the answer is non-empty and the citation gate verified at least three
#     path:line citations with none left wrong (live_endpoint_validate.py)
# Writes a one-line summary (and the raw result) to $OUT_DIR for the CI log.
#
# Usage: scripts/live_endpoint_check.sh [path/to/selfware]
# Env:   SELFWARE_ENDPOINT (default https://llm.selfware.design/v1)
#        SELFWARE_CONFIG   (default selfware-llm-selfware-design.toml)
#        LIVE_WALL_SECS    (default 900)
#        OUT_DIR           (default ./live-endpoint-out)
set -euo pipefail

BIN="${1:-target/release/selfware}"
ENDPOINT="${SELFWARE_ENDPOINT:-https://llm.selfware.design/v1}"
CONFIG="${SELFWARE_CONFIG:-selfware-llm-selfware-design.toml}"
WALL="${LIVE_WALL_SECS:-900}"
OUT_DIR="${OUT_DIR:-live-endpoint-out}"
mkdir -p "$OUT_DIR"

fail() { echo "LIVE CHECK FAILED: $*" | tee "$OUT_DIR/summary.txt"; exit 1; }

[ -x "$BIN" ] || fail "binary not found at $BIN"
BIN="$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")"
[ -f "$CONFIG" ] || fail "config not found at $CONFIG"

# 1. Reachability: a failure here is a failure, never a skip.
if ! curl -fsS -m 20 "$ENDPOINT/models" > "$OUT_DIR/models.json"; then
    fail "endpoint $ENDPOINT/models not reachable"
fi

# 2. A small, deterministic read-only review in a throwaway copy of the repo.
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
git archive HEAD | tar -x -C "$WORK"
cp "$CONFIG" "$WORK/live.toml"
(cd "$WORK" && git init -q -b main && git add -A && git -c user.email=ci@local -c user.name=ci commit -qm fixture)

PROMPT='Read src/agent/llm_wait.rs. Report the three most important functions or types it defines. For each, give one sentence on what it does and cite it as `name` (path:line) using the exact line where it is defined. Do not modify any files.'

"$BIN" trust "$WORK/live.toml" -q > /dev/null 2>&1 || true
set +e
(cd "$WORK" && "$BIN" --config live.toml --mode yolo --no-color \
    --output-format json --max-wall-secs "$WALL" -p "$PROMPT") \
    > "$OUT_DIR/result.json" 2> "$OUT_DIR/stderr.txt"
CODE=$?
set -e

# 3. Structured checks on the result object (scripts/live_endpoint_validate.py:
#    exactly one result, non-empty answer, >=3 verified citations, none wrong).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 "$SCRIPT_DIR/live_endpoint_validate.py" "$OUT_DIR/result.json" "$CODE" \
    > "$OUT_DIR/summary.txt" || { cat "$OUT_DIR/summary.txt"; exit 1; }
cat "$OUT_DIR/summary.txt"
