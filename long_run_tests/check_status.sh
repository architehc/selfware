#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

find_latest_run() {
  ls -td "$ROOT_DIR"/long_run_tests/system_test_8hr_v4_* 2>/dev/null | head -1 || true
}

RUN_DIR="${1:-$(find_latest_run)}"
if [[ -z "$RUN_DIR" || ! -d "$RUN_DIR" ]]; then
  echo "No v4 8-hour test run found"
  exit 1
fi

STATUS_FILE="$RUN_DIR/STATUS.md"
STATE_FILE="$RUN_DIR/current_state.env"
PID_FILE="$RUN_DIR/run.pid"
ALL_RESULTS="$RUN_DIR/ALL_RESULTS.md"

RUN_STATUS="UNKNOWN"
RUN_PID=""
if [[ -f "$PID_FILE" ]]; then
  RUN_PID="$(cat "$PID_FILE" 2>/dev/null || true)"
  if [[ -n "$RUN_PID" ]] && kill -0 "$RUN_PID" 2>/dev/null; then
    RUN_STATUS="RUNNING"
  else
    RUN_STATUS="FINISHED/STOPPED"
  fi
fi

echo "=== 8-HOUR TEST STATUS - $(date) ==="
echo "Run: $RUN_DIR"
if [[ -n "$RUN_PID" ]]; then
  echo "Process: $RUN_STATUS (PID $RUN_PID)"
else
  echo "Process: $RUN_STATUS"
fi
echo

if [[ -f "$STATUS_FILE" ]]; then
  sed -n '1,220p' "$STATUS_FILE"
  exit 0
fi

if [[ -f "$STATE_FILE" ]]; then
  echo "--- State ---"
  sed -n '1,120p' "$STATE_FILE"
  echo
fi

if [[ -f "$ALL_RESULTS" ]]; then
  echo "--- Results So Far ---"
  sed -n '1,220p' "$ALL_RESULTS"
  echo
  GREENS=$(grep -c "| GREEN |" "$ALL_RESULTS" 2>/dev/null || true)
  PARTIALS=$(grep -c "| PARTIAL |" "$ALL_RESULTS" 2>/dev/null || true)
  TOTAL=$(grep -Ec '^\| C[0-9]+ \| R[0-9]+ \|' "$ALL_RESULTS" 2>/dev/null || true)
  echo "Score: $GREENS GREEN, $PARTIALS PARTIAL / $TOTAL total"
fi
