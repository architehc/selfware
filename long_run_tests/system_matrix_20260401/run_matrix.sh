#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MATRIX_DIR="$ROOT_DIR/long_run_tests/system_matrix_20260401"
SOURCE_DIR="$ROOT_DIR/long_run_tests/guided_scheduler_lab"
BINARY_PATH="$ROOT_DIR/target/debug/selfware"
CONFIG_PATH="$ROOT_DIR/selfware.toml"
TIMEOUT_SECS="${TIMEOUT_SECS:-180}"

mkdir -p "$MATRIX_DIR/scenarios"

for prompt_path in "$MATRIX_DIR"/prompts/*.md; do
  scenario_name="$(basename "$prompt_path" .md)"
  scenario_dir="$MATRIX_DIR/scenarios/$scenario_name"

  rm -rf "$scenario_dir"
  mkdir -p "$scenario_dir/src" "$scenario_dir/tests"

  cp "$SOURCE_DIR/Cargo.toml" "$scenario_dir/"
  cp "$SOURCE_DIR/LONG_RUN_BRIEF.md" "$scenario_dir/"
  cp "$SOURCE_DIR/RUN_NOTES.md" "$scenario_dir/"
  cp "$SOURCE_DIR/src/"*.rs "$scenario_dir/src/"
  cp "$SOURCE_DIR/tests/guided_flow.rs" "$scenario_dir/tests/"
done

pids=()

for prompt_path in "$MATRIX_DIR"/prompts/*.md; do
  scenario_name="$(basename "$prompt_path" .md)"
  scenario_dir="$MATRIX_DIR/scenarios/$scenario_name"
  prompt="$(cat "$prompt_path")"
  log_path="$scenario_dir/run.log"
  exit_path="$scenario_dir/exit_code.txt"

  (
    set +e
    timeout "$TIMEOUT_SECS" \
      "$BINARY_PATH" \
      --config "$CONFIG_PATH" \
      -C "$scenario_dir" \
      --daemon \
      --ascii \
      --no-color \
      -p "$prompt" \
      >"$log_path" 2>&1
    status=$?
    printf '%s\n' "$status" >"$exit_path"
    exit 0
  ) &

  pids+=("$!")
done

for pid in "${pids[@]}"; do
  wait "$pid"
done

"$MATRIX_DIR/summarize_matrix.sh"
