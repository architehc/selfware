#!/usr/bin/env bash
# Live check against a real OpenAI-compatible endpoint (default:
# llm.selfware.design). Runs a short read-only review task headless and checks
# the structured outcome, not the model's prose:
#   - the endpoint answers /models (an unreachable endpoint FAILS; it is never
#     reported as a skip — AGENTS.md rule 3)
#   - the run ends with exactly one JSON result object whose exit_status
#     equals the process exit code
#   - the answer carries path:line citations and the citation gate verified at
#     least one of them
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

# 3. Structured checks on the result object.
python3 - "$OUT_DIR/result.json" "$CODE" > "$OUT_DIR/summary.txt" <<'PY' || { cat "$OUT_DIR/summary.txt"; exit 1; }
import json, sys
path, code = sys.argv[1], int(sys.argv[2])
text = open(path).read().strip()
result = None
for candidate in ([text] + text.splitlines()[::-1]) if text else []:
    try:
        result = json.loads(candidate)
        break
    except json.JSONDecodeError:
        continue
if not isinstance(result, dict):
    print("LIVE CHECK FAILED: no JSON result object on stdout"); sys.exit(1)
problems = []
if result.get("exit_status") != code:
    problems.append(f"exit_status {result.get('exit_status')} != process exit {code}")
if code != 0:
    problems.append(f"run exited {code}")
g = result.get("grounding") or {}
verified = g.get("verified", 0)
total = g.get("total", 0)
if total == 0:
    problems.append("no checkable citations in the answer")
elif verified == 0:
    problems.append(f"0 of {total} citations verified")
line = (f"exit={code} citations: {verified}/{total} verified, "
        f"wrong={g.get('wrong_line', 0)} correction_rounds={g.get('correction_rounds', 0)}")
if problems:
    print("LIVE CHECK FAILED: " + "; ".join(problems) + " | " + line); sys.exit(1)
print("LIVE CHECK PASSED: " + line)
PY
cat "$OUT_DIR/summary.txt"
