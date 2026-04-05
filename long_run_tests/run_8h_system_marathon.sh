#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LONG_RUN_DIR="$ROOT_DIR/long_run_tests"
RUN_ID="${RUN_ID_OVERRIDE:-marathon_$(date +%Y%m%d_%H%M%S)}"
RUN_DIR="$LONG_RUN_DIR/$RUN_ID"
STATUS_INTERVAL_SECS="${STATUS_INTERVAL_SECS:-900}"
DURATION_HOURS="${DURATION_HOURS:-8}"
ENDPOINT="${ENDPOINT:-https://crazyshit.ngrok.io/v1}"
SKIP_COMPLETED_PROJECTS="${SKIP_COMPLETED_PROJECTS:-1}"

mkdir -p "$RUN_DIR"

STATUS_FILE="$RUN_DIR/STATUS.md"
STATE_FILE="$RUN_DIR/current_state.env"
MASTER_SUMMARY="$RUN_DIR/MASTER_SUMMARY.md"
EVENT_LOG="$RUN_DIR/events.log"
RUN_LOG="$RUN_DIR/marathon.log"
PID_FILE="$RUN_DIR/pid"
MONITOR_PID_FILE="$RUN_DIR/monitor.pid"

START_TS="$(date +%s)"
END_TS="$((START_TS + DURATION_HOURS * 3600))"

cat >"$MASTER_SUMMARY" <<'EOF'
# 8-Hour System Marathon Summary

| Round | Batch | Results Dir | Status |
|---|---|---|---|
EOF

write_state() {
  {
    printf 'RUN_ID=%q\n' "$RUN_ID"
    printf 'RUN_DIR=%q\n' "$RUN_DIR"
    printf 'START_TS=%q\n' "$START_TS"
    printf 'END_TS=%q\n' "$END_TS"
    printf 'ROUND_INDEX=%q\n' "${ROUND_INDEX:-0}"
    printf 'ACTIVE_BATCH=%q\n' "${ACTIVE_BATCH:-idle}"
    printf 'ACTIVE_RESULTS_DIR=%q\n' "${ACTIVE_RESULTS_DIR:-}"
    printf 'ACTIVE_PID=%q\n' "${ACTIVE_PID:-}"
    printf 'ACTIVE_STARTED_TS=%q\n' "${ACTIVE_STARTED_TS:-}"
    printf 'LAST_EVENT=%q\n' "${LAST_EVENT:-}"
  } >"$STATE_FILE"
}

snapshot_status() {
  local now remaining active_duration active_pid_count tmp_status_file
  now="$(date +%s)"
  remaining="$((END_TS - now))"
  if (( remaining < 0 )); then
    remaining=0
  fi

  if [[ -n "${ACTIVE_STARTED_TS:-}" ]]; then
    active_duration="$((now - ACTIVE_STARTED_TS))"
  else
    active_duration=0
  fi

  active_pid_count="$(pgrep -fc '/home/ivo/selfware/target/debug/selfware' || true)"
  tmp_status_file="$(mktemp "${STATUS_FILE}.XXXXXX")"

  {
    printf '# 8-Hour System Marathon Status\n\n'
    printf -- '- Run dir: `%s`\n' "$RUN_DIR"
    printf -- '- Started: `%s`\n' "$(date -d "@$START_TS" '+%Y-%m-%d %H:%M:%S %Z')"
    printf -- '- Deadline: `%s`\n' "$(date -d "@$END_TS" '+%Y-%m-%d %H:%M:%S %Z')"
    printf -- '- Seconds remaining: `%s`\n' "$remaining"
    printf -- '- Current round: `%s`\n' "${ROUND_INDEX:-0}"
    printf -- '- Active batch: `%s`\n' "${ACTIVE_BATCH:-idle}"
    printf -- '- Active batch elapsed seconds: `%s`\n' "$active_duration"
    printf -- '- Active selfware processes: `%s`\n' "$active_pid_count"
    printf -- '- Last event: `%s`\n' "${LAST_EVENT:-none}"
    printf '\n'

    if [[ -n "${ACTIVE_RESULTS_DIR:-}" && -f "${ACTIVE_RESULTS_DIR}/SUMMARY.md" ]]; then
      printf '## Active Summary\n\n'
      sed -n '1,120p' "${ACTIVE_RESULTS_DIR}/SUMMARY.md"
      printf '\n'
    fi

    if [[ -f "$MASTER_SUMMARY" ]]; then
      printf '## Completed Batches\n\n'
      sed -n '1,160p' "$MASTER_SUMMARY"
      printf '\n'
    fi
  } >"$tmp_status_file"

  mv "$tmp_status_file" "$STATUS_FILE"
}

monitor_loop() {
  while true; do
    if [[ -f "$STATE_FILE" ]]; then
      # shellcheck disable=SC1090
      source "$STATE_FILE"
    fi
    snapshot_status
    if (( $(date +%s) >= END_TS )); then
      break
    fi
    sleep "$STATUS_INTERVAL_SECS"
  done
}

append_event() {
  LAST_EVENT="$1"
  printf '[%s] %s\n' "$(date '+%Y-%m-%d %H:%M:%S %Z')" "$1" | tee -a "$EVENT_LOG" >/dev/null
  write_state
  snapshot_status
}

LAST_BATCH_SUMMARY_STATUS="unknown"

run_batch() {
  local batch_name="$1"
  local results_dir="$2"
  shift 2

  ACTIVE_BATCH="$batch_name"
  ACTIVE_RESULTS_DIR="$results_dir"
  ACTIVE_STARTED_TS="$(date +%s)"
  ACTIVE_PID=""
  append_event "Starting ${batch_name} at ${results_dir}"

  (
    set +e
    "$@"
    printf '%s\n' "$?" >"${results_dir}.exit_code"
  ) &
  ACTIVE_PID="$!"
  write_state

  wait "$ACTIVE_PID"
  ACTIVE_PID=""
  local batch_exit=0
  if [[ -f "${results_dir}.exit_code" ]]; then
    batch_exit="$(cat "${results_dir}.exit_code")"
  fi

  local summary_status="no_summary"
  if [[ -f "$results_dir/SUMMARY.md" ]]; then
    if grep -q 'green_but_test_tampered' "$results_dir/SUMMARY.md"; then
      summary_status="green_with_test_tampering"
    elif grep -q 'needs_work\|edited_but_failing' "$results_dir/SUMMARY.md"; then
      summary_status="needs_work"
    else
      summary_status="complete"
    fi
  elif [[ "$batch_exit" != "0" ]]; then
    summary_status="runner_failed"
  fi
  LAST_BATCH_SUMMARY_STATUS="$summary_status"

  printf '| %s | %s | %s | %s |\n' \
    "$ROUND_INDEX" \
    "$batch_name" \
    "$results_dir" \
    "$summary_status" \
    >>"$MASTER_SUMMARY"

  append_event "Finished ${batch_name} exit=${batch_exit} status=${summary_status}"
  write_state
  snapshot_status
}

check_endpoint() {
  curl -s --connect-timeout 5 "$ENDPOINT/models" | grep -q 'txn545/Qwen3.5-122B-A10B-NVFP4'
}

all_projects_completed() {
  local entry batch_type project_name project_key
  for entry in "${PROJECT_QUEUE[@]}"; do
    batch_type="${entry%%:*}"
    project_name="${entry#*:}"
    project_key="${batch_type}:${project_name}"
    if [[ "${COMPLETED_PROJECTS[$project_key]:-0}" != "1" ]]; then
      return 1
    fi
  done
  return 0
}

chmod +x "$ROOT_DIR/long_run_tests/run_greenfield_e2e_batch.sh"
chmod +x "$ROOT_DIR/long_run_tests/run_template_rust_batch.sh"

printf '%s\n' "$$" >"$PID_FILE"
write_state
monitor_loop &
MONITOR_PID="$!"
printf '%s\n' "$MONITOR_PID" >"$MONITOR_PID_FILE"

cleanup_on_exit() {
  kill "$MONITOR_PID" 2>/dev/null || true
  write_state
  snapshot_status
}

shutdown_on_signal() {
  append_event "Received stop signal"
  exit 130
}

trap cleanup_on_exit EXIT
trap shutdown_on_signal INT TERM

append_event "Marathon started"

ROUND_INDEX=0
declare -A COMPLETED_PROJECTS=()
PROJECT_QUEUE=(
  "greenfield:slugger"
  "greenfield:money_splitter"
  "greenfield:bounded_queue"
  "template:viz_ascii_table"
  "template:hard_scheduler"
  "template:hard_event_bus"
  "template:viz_svg_chart"
  "template:viz_maze_gen"
)

while (( $(date +%s) < END_TS )); do
  if ! check_endpoint; then
    append_event "Endpoint health check failed; sleeping 60s"
    sleep 60
    continue
  fi

  if [[ "$SKIP_COMPLETED_PROJECTS" == "1" ]] && all_projects_completed; then
    append_event "All queued projects completed; ending marathon early"
    break
  fi

  for entry in "${PROJECT_QUEUE[@]}"; do
    if (( $(date +%s) >= END_TS )); then
      break 2
    fi

    batch_type="${entry%%:*}"
    project_name="${entry#*:}"
    project_key="${batch_type}:${project_name}"

    if [[ "$SKIP_COMPLETED_PROJECTS" == "1" && "${COMPLETED_PROJECTS[$project_key]:-0}" == "1" ]]; then
      continue
    fi

    ROUND_INDEX="$((ROUND_INDEX + 1))"

    case "$batch_type" in
      greenfield)
        batch_results_dir="$RUN_DIR/round_$(printf '%03d' "$ROUND_INDEX")_greenfield_${project_name}"
        run_batch \
          "greenfield[$project_name]" \
          "$batch_results_dir" \
          env \
            RESULTS_DIR_OVERRIDE="$batch_results_dir" \
            PROJECTS="$project_name" \
            PASS1_TIMEOUT=180 \
            PASS2_TIMEOUT=150 \
            "$ROOT_DIR/long_run_tests/run_greenfield_e2e_batch.sh"
        ;;
      template)
        batch_results_dir="$RUN_DIR/round_$(printf '%03d' "$ROUND_INDEX")_template_${project_name}"
        run_batch \
          "template[$project_name]" \
          "$batch_results_dir" \
          env \
            RESULTS_DIR_OVERRIDE="$batch_results_dir" \
            TEMPLATE_PROJECTS="$project_name" \
            PASS1_TIMEOUT=720 \
            PASS2_TIMEOUT=480 \
            "$ROOT_DIR/long_run_tests/run_template_rust_batch.sh"
        ;;
    esac

    if [[ "$SKIP_COMPLETED_PROJECTS" == "1" && "$LAST_BATCH_SUMMARY_STATUS" == "complete" ]]; then
      COMPLETED_PROJECTS["$project_key"]=1
      append_event "Completed ${project_key}; removing it from the retry queue"
    fi
  done
done

ACTIVE_BATCH="idle"
ACTIVE_RESULTS_DIR=""
ACTIVE_STARTED_TS=""
append_event "Marathon reached deadline"
write_state
snapshot_status
kill "$MONITOR_PID" 2>/dev/null || true
