#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INTERVAL="${INTERVAL:-30}"

find_latest_run() {
  local dir pid_file pid

  while IFS= read -r dir; do
    pid_file="$dir/run.pid"
    if [[ -f "$pid_file" ]]; then
      pid="$(cat "$pid_file" 2>/dev/null || true)"
      if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        printf '%s\n' "$dir"
        return 0
      fi
    fi
  done < <(find "$ROOT_DIR/long_run_tests" -maxdepth 1 -type d -name 'system_test_8hr_v4_*' | sort -r)

  find "$ROOT_DIR/long_run_tests" -maxdepth 1 -type d -name 'system_test_8hr_v4_*' | sort -r | head -1 || true
}

WATCH_MODE=0
if [[ "${1:-}" == "--watch" ]]; then
  WATCH_MODE=1
  shift
fi

RUN_DIR="${1:-$(find_latest_run)}"
if [[ -z "$RUN_DIR" || ! -d "$RUN_DIR" ]]; then
  echo "No v4 8-hour test run found"
  exit 1
fi

render_snapshot() {
  local status_file="$RUN_DIR/STATUS.md"
  local final_report="$RUN_DIR/FINAL_REPORT.md"
  local events="$RUN_DIR/events.log"

  echo "============================================================"
  echo "  8-HOUR TEST MONITOR - $(date)"
  echo "============================================================"
  echo "Run: $RUN_DIR"
  echo

  if [[ -f "$status_file" ]]; then
    sed -n '1,240p' "$status_file"
  elif [[ -f "$final_report" ]]; then
    sed -n '1,240p' "$final_report"
  else
    echo "No STATUS.md or FINAL_REPORT.md present yet."
  fi

  if [[ -f "$events" ]]; then
    echo
    echo "--- Recent Events ---"
    tail -10 "$events"
  fi
}

if (( WATCH_MODE == 1 )); then
  while true; do
    clear
    render_snapshot
    sleep "$INTERVAL"
  done
else
  render_snapshot
fi
