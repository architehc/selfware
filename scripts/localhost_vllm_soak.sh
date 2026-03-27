#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SELFWARE_BIN="${SELFWARE_BIN:-$ROOT_DIR/target/release/selfware}"
SELFWARE_CONFIG_FILE="${SELFWARE_CONFIG_FILE:-$ROOT_DIR/selfware-27b-concurrency16.toml}"
ENDPOINT="${SELFWARE_ENDPOINT:-http://localhost:8000/v1}"
SOAK_HOURS="${SOAK_HOURS:-8}"
HEALTH_INTERVAL_SECS="${HEALTH_INTERVAL_SECS:-900}"
RUN_TIMEOUT_SECS="${RUN_TIMEOUT_SECS:-1800}"
MODEL="${SELFWARE_MODEL:-qwen3.5-27b}"
LOG_ROOT="${SOAK_LOG_ROOT:-/tmp/selfware-soak-$(date +%Y%m%d-%H%M%S)}"
WORKTREE_DIR="$LOG_ROOT/worktree"
MAIN_LOG="$LOG_ROOT/soak.log"
SNAPSHOT_LOG="$LOG_ROOT/health_snapshots.log"
WORKTREE_LOG="$LOG_ROOT/worktree.log"

SOAK_SECONDS=$((SOAK_HOURS * 60 * 60))
START_EPOCH=$(date +%s)
DEADLINE_EPOCH=$((START_EPOCH + SOAK_SECONDS))

TASKS=(
  "Perform a deep code review of the selfware repository with emphasis on runtime stability, async control flow, and long-run failure modes. Do not edit files. Return only concrete findings with file references and a short risk assessment."
  "Inspect the visual verification and computer-control paths in the current repository. Do not edit files. Focus on stuck-loop detection, window targeting, screenshot capture, and recovery behavior. Return a concise report."
  "Review configuration and endpoint handling for the localhost vLLM setup. Do not edit files. Look for config drift, invalid defaults, and any issues that could shorten an 8-hour soak run."
  "Audit test coverage and identify missing regressions for long-running agent execution. Do not edit files. Prioritize the tests that would catch the most severe soak failures."
  "Analyze performance bottlenecks in the agent loop, tool dispatch, and visual verification flow. Do not edit files. Focus on repeated work, blocking IO, and unnecessary context growth."
  "Perform a targeted review of error handling and recovery logic. Do not edit files. Focus on how the agent recovers from tool failures, visual failures, and endpoint interruptions."
)

health_snapshot() {
  local ts
  ts="$(date -Iseconds)"
  {
    echo "=== ${ts} ==="
    echo "[endpoint] ${ENDPOINT}"

    if curl -fsS --max-time 20 "${ENDPOINT}/health" >/dev/null 2>&1; then
      echo "[health] ok"
    else
      echo "[health] unavailable"
    fi

    if curl -fsS --max-time 20 "${ENDPOINT}/models" >/dev/null 2>&1; then
      echo "[models] ok"
      curl -fsS --max-time 20 "${ENDPOINT}/models" 2>/dev/null || true
    else
      echo "[models] unavailable"
    fi

    if command -v nvidia-smi >/dev/null 2>&1; then
      echo "[gpu] nvidia-smi"
      nvidia-smi --query-gpu=index,name,utilization.gpu,utilization.memory,memory.used,memory.total \
        --format=csv,noheader,nounits
    else
      echo "[gpu] nvidia-smi not available"
    fi
    echo
  } >> "$SNAPSHOT_LOG"
}

monitor_loop() {
  while true; do
    sleep "$HEALTH_INTERVAL_SECS"
    health_snapshot
  done
}

build_selfware() {
  if [ -x "$SELFWARE_BIN" ]; then
    return 0
  fi

  echo "[build] compiling release binary" | tee -a "$MAIN_LOG"
  (cd "$ROOT_DIR" && cargo build --release --bin selfware) | tee -a "$MAIN_LOG"
}

ensure_endpoint() {
  if ! curl -fsS --max-time 20 "${ENDPOINT}/models" >/dev/null 2>&1; then
    echo "[error] endpoint is not responding at ${ENDPOINT}" | tee -a "$MAIN_LOG"
    exit 1
  fi
}

setup_worktree() {
  if [ -d "$WORKTREE_DIR" ]; then
    echo "[setup] removing stale worktree directory: $WORKTREE_DIR" | tee -a "$MAIN_LOG"
    rm -rf "$WORKTREE_DIR"
  fi

  echo "[setup] creating isolated worktree at $WORKTREE_DIR" | tee -a "$MAIN_LOG"
  git -C "$ROOT_DIR" worktree add --detach "$WORKTREE_DIR" HEAD | tee -a "$WORKTREE_LOG"
}

run_task_cycle() {
  local cycle="$1"
  local task="$2"
  local cycle_log="$LOG_ROOT/cycle-$(printf '%04d' "$cycle").log"
  local elapsed

  elapsed=$(( $(date +%s) - START_EPOCH ))
  printf '[cycle %04d] elapsed=%ss task=%s\n' "$cycle" "$elapsed" "$task" | tee -a "$MAIN_LOG"

  if timeout --preserve-status --kill-after=120s "$RUN_TIMEOUT_SECS" \
    "$SELFWARE_BIN" \
    --config "$SELFWARE_CONFIG_FILE" \
    --workdir "$WORKTREE_DIR" \
    --yolo \
    run "$task" 2>&1 | tee "$cycle_log"; then
    printf '[cycle %04d] completed successfully\n' "$cycle" | tee -a "$MAIN_LOG"
    return 0
  fi

  local rc=${PIPESTATUS[0]}
  printf '[cycle %04d] exited with code %s\n' "$cycle" "$rc" | tee -a "$MAIN_LOG"
  return 0
}

cleanup() {
  local rc=$?
  if [ -n "${MONITOR_PID:-}" ] && kill -0 "$MONITOR_PID" 2>/dev/null; then
    kill "$MONITOR_PID" 2>/dev/null || true
    wait "$MONITOR_PID" 2>/dev/null || true
  fi

  health_snapshot || true
  {
    echo "=== finished $(date -Iseconds) ==="
    echo "exit_code=$rc"
    echo "log_root=$LOG_ROOT"
  } >> "$MAIN_LOG"
}
trap cleanup EXIT INT TERM

mkdir -p "$LOG_ROOT"
: > "$MAIN_LOG"
: > "$SNAPSHOT_LOG"
: > "$WORKTREE_LOG"

{
  echo "selfware localhost vllm soak"
  echo "root_dir=$ROOT_DIR"
  echo "binary=$SELFWARE_BIN"
  echo "config=$SELFWARE_CONFIG_FILE"
  echo "endpoint=$ENDPOINT"
  echo "model=$MODEL"
  echo "soak_hours=$SOAK_HOURS"
  echo "health_interval_secs=$HEALTH_INTERVAL_SECS"
  echo "run_timeout_secs=$RUN_TIMEOUT_SECS"
  echo "log_root=$LOG_ROOT"
} | tee -a "$MAIN_LOG"

build_selfware
ensure_endpoint
setup_worktree
health_snapshot
monitor_loop &
MONITOR_PID=$!

cycle=1
while [ "$(date +%s)" -lt "$DEADLINE_EPOCH" ]; do
  task_index=$(( (cycle - 1) % ${#TASKS[@]} ))
  run_task_cycle "$cycle" "${TASKS[$task_index]}"
  cycle=$((cycle + 1))
done

echo "[done] soak deadline reached" | tee -a "$MAIN_LOG"
