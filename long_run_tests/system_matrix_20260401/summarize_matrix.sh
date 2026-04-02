#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MATRIX_DIR="$ROOT_DIR/long_run_tests/system_matrix_20260401"

printf '%-28s %-6s %-8s %-8s %-8s %-10s %-10s\n' \
  "scenario" "exit" "steps" "edits" "notes" "tool_search" "cant_access"

for scenario_dir in "$MATRIX_DIR"/scenarios/*; do
  scenario_name="$(basename "$scenario_dir")"
  exit_code="$(tr -d '\n' < "$scenario_dir/exit_code.txt" 2>/dev/null || printf 'na')"
  log_path="$scenario_dir/run.log"

  steps="$(rg -o 'Step [0-9]+' "$log_path" 2>/dev/null | tail -n 1 | awk '{print $2}' || true)"
  if [[ -z "${steps:-}" ]]; then
    steps="0"
  fi

  edits="no"
  if rg -q 'file_edit|file_write' "$log_path" 2>/dev/null; then
    edits="yes"
  fi

  notes="no"
  if ! cmp -s \
    "$ROOT_DIR/long_run_tests/guided_scheduler_lab/RUN_NOTES.md" \
    "$scenario_dir/RUN_NOTES.md"; then
    notes="yes"
  fi

  tool_search="no"
  if rg -q 'tool_search' "$log_path" 2>/dev/null; then
    tool_search="yes"
  fi

  cant_access="no"
  if rg -q "I cannot execute|do not have access|lack filesystem access" "$log_path" 2>/dev/null; then
    cant_access="yes"
  fi

  printf '%-28s %-6s %-8s %-8s %-8s %-10s %-10s\n' \
    "$scenario_name" "$exit_code" "$steps" "$edits" "$notes" "$tool_search" "$cant_access"
done
