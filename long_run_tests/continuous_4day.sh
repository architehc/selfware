#!/usr/bin/env bash
# 4-day continuous test-improve loop for selfware harness.
#
# Workflow per cycle:
#   1. Wait for any running test to finish
#   2. Check if source changed → cargo build --release
#   3. Preflight: bash -n on test script + 2-min smoke run
#   4. Launch 8-hour test (latest v* script)
#   5. Generate cross-run comparison report
#   6. Sleep until test finishes, then repeat
#
# Endpoint recovery: if endpoint is down, wait and retry every 5 min.
# Safe to Ctrl-C — generates summary on exit.

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LONG_RUN_DIR="$ROOT_DIR/long_run_tests"
SELFWARE="$ROOT_DIR/target/release/selfware"
ENDPOINT="${ENDPOINT:-https://crazyshit.ngrok.io/v1}"
MODEL="${MODEL:-txn545/Qwen3.5-122B-A10B-NVFP4}"
TEMPLATES="$ROOT_DIR/system_tests/projecte2e/templates"
DURATION_DAYS="${DURATION_DAYS:-4}"
DURATION_SECS=$((DURATION_DAYS * 86400))
LOG_FILE="$LONG_RUN_DIR/continuous_4day_$(date +%Y%m%d_%H%M%S).log"
SUMMARY_FILE="$LONG_RUN_DIR/CONTINUOUS_SUMMARY.md"
CHECK_INTERVAL=120  # seconds between status checks while test runs
SMOKE_TIMEOUT=120   # 2 minutes for smoke test

START_TIME=$(date +%s)
CYCLE=0
LAST_BUILD_HASH=""

log() { printf '[%s] %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$*" | tee -a "$LOG_FILE"; }

elapsed_human() {
  local d=$(( ($(date +%s) - START_TIME) ))
  printf '%dd %dh%02dm' $((d/86400)) $(((d%86400)/3600)) $(((d%3600)/60))
}

time_remaining() {
  local r=$(( DURATION_SECS - ($(date +%s) - START_TIME) ))
  (( r > 0 )) && echo "$r" || echo 0
}

# ── Find latest test script ──
latest_test_script() {
  ls -t "$LONG_RUN_DIR"/run_8hour_system_test_v*.sh 2>/dev/null | head -1
}

# ── Endpoint health ──
wait_for_endpoint() {
  while ! curl -fsS --connect-timeout 5 "$ENDPOINT/models" 2>/dev/null | grep -q "$MODEL"; do
    log "ENDPOINT DOWN — waiting 5 min..."
    sleep 300
    if (( $(time_remaining) <= 0 )); then
      log "Time expired while waiting for endpoint."
      return 1
    fi
  done
  log "Endpoint OK: $MODEL"
  return 0
}

# ── Rebuild if source changed ──
maybe_rebuild() {
  local current_hash
  current_hash="$(cd "$ROOT_DIR" && git rev-parse HEAD 2>/dev/null || echo none)"

  if [[ "$current_hash" == "$LAST_BUILD_HASH" ]]; then
    log "No source changes (HEAD=$current_hash), skipping rebuild."
    return 0
  fi

  log "Source changed: $LAST_BUILD_HASH → $current_hash — rebuilding..."
  if (cd "$ROOT_DIR" && cargo build --release 2>&1 | tail -5 | tee -a "$LOG_FILE"); then
    LAST_BUILD_HASH="$current_hash"
    log "Build OK."
  else
    log "BUILD FAILED — using previous binary."
    return 1
  fi
}

# ── Preflight: syntax check + smoke run ──
preflight() {
  local script="$1"

  # Syntax check
  if ! bash -n "$script" 2>&1; then
    log "PREFLIGHT FAIL: $script has syntax errors."
    return 1
  fi
  log "Preflight syntax OK: $(basename "$script")"

  # 2-min smoke run: one greenfield project
  local smoke_dir
  smoke_dir="$LONG_RUN_DIR/smoke_$(date +%Y%m%d_%H%M%S)"
  mkdir -p "$smoke_dir/src"
  cat > "$smoke_dir/Cargo.toml" << 'TOML'
[package]
name = "smoke-test"
version = "0.1.0"
edition = "2021"
TOML
  echo '// implement here' > "$smoke_dir/src/lib.rs"
  cat > "$smoke_dir/selfware.toml" << TOML
endpoint = "$ENDPOINT"
model = "$MODEL"
max_tokens = 16384
context_length = 262144
temperature = 0.6
[safety]
allowed_paths = ["./**", "/tmp/**"]
[agent]
max_iterations = 15
step_timeout_secs = 120
native_function_calling = false
streaming = true
[continuous_work]
enabled = false
[extra_body]
chat_template_kwargs = { enable_thinking = false }
[retry]
max_retries = 3
base_delay_ms = 2000
max_delay_ms = 30000
TOML
  (cd "$smoke_dir" && git init -q && git add -A && git commit -q -m init) 2>/dev/null

  log "Smoke test: running 15-step calculator task (${SMOKE_TIMEOUT}s timeout)..."
  if timeout "$SMOKE_TIMEOUT" "$SELFWARE" \
    -c "$smoke_dir/selfware.toml" \
    -C "$smoke_dir" \
    --yolo --ascii --no-color \
    -p "Create add, subtract, multiply, divide functions in src/lib.rs. divide returns Result for division by zero. Write 4 tests. Run cargo test." \
    > "$smoke_dir/smoke.log" 2>&1; then
    log "Smoke test completed (exit 0)."
  else
    local ec=$?
    log "Smoke test exited with code $ec (timeout=124 is OK)."
  fi

  # Check: did it write anything?
  local lines
  lines=$(wc -l < "$smoke_dir/src/lib.rs" 2>/dev/null || echo 0)
  if (( lines > 3 )); then
    log "Smoke PASSED: src/lib.rs has $lines lines."
    rm -rf "$smoke_dir"
    return 0
  else
    log "Smoke FAILED: src/lib.rs has only $lines lines — agent may not be generating code."
    # Keep smoke dir for debugging
    return 1
  fi
}

# ── Find PID of running test ──
find_running_test() {
  local latest_dir
  latest_dir="$(ls -td "$LONG_RUN_DIR"/system_test_8hr_v* 2>/dev/null | head -1)"
  if [[ -n "$latest_dir" && -f "$latest_dir/run.pid" ]]; then
    local pid
    pid="$(cat "$latest_dir/run.pid")"
    if kill -0 "$pid" 2>/dev/null; then
      echo "$pid"
      return 0
    fi
  fi
  echo ""
  return 1
}

# ── Wait for running test to finish ──
wait_for_test() {
  local pid="$1"
  log "Waiting for test PID $pid to finish..."
  while kill -0 "$pid" 2>/dev/null; do
    sleep "$CHECK_INTERVAL"
    if (( $(time_remaining) <= 0 )); then
      log "4-day window expired. Killing test PID $pid."
      kill "$pid" 2>/dev/null || true
      sleep 5
      return
    fi
  done
  log "Test PID $pid finished."
}

# ── Launch new test ──
launch_test() {
  local script="$1"
  CYCLE=$((CYCLE + 1))
  log "=== CYCLE $CYCLE — launching $(basename "$script") ==="

  # Calculate remaining hours for this run (cap at 8h, leave 30min buffer)
  local remaining
  remaining=$(time_remaining)
  local run_hours=$(( remaining / 3600 ))
  (( run_hours > 8 )) && run_hours=8
  (( run_hours < 1 )) && { log "Less than 1h remaining, stopping."; return 1; }

  DURATION_HOURS="$run_hours" nohup bash "$script" >> "$LOG_FILE" 2>&1 &
  local pid=$!
  log "Test launched: PID=$pid, duration=${run_hours}h, script=$(basename "$script")"
  echo "$pid"
}

# ── Generate cross-run comparison report ──
generate_report() {
  log "Generating cross-run comparison report..."

  cat > "$SUMMARY_FILE" << 'EOF'
# Continuous Test Summary

| Run | Total | GREEN | PARTIAL | UNVERIFIED | COMPILES | WROTE | FAIL | GREEN% |
|-----|-------|-------|---------|------------|----------|-------|------|--------|
EOF

  for dir in $(ls -t "$LONG_RUN_DIR"/system_test_8hr_v* -d 2>/dev/null | head -20); do
    [[ -f "$dir/ALL_RESULTS.md" ]] || continue
    local name total green partial unverified compiles wrote fail rate
    name="$(basename "$dir" | sed 's/system_test_8hr_//')"
    total=$(grep -c '^| C' "$dir/ALL_RESULTS.md" 2>/dev/null || echo 0)
    (( total == 0 )) && continue
    green=$(grep -c '| GREEN |' "$dir/ALL_RESULTS.md" 2>/dev/null || echo 0)
    partial=$(grep -c '| PARTIAL |' "$dir/ALL_RESULTS.md" 2>/dev/null || echo 0)
    unverified=$(grep -c '| UNVERIFIED |' "$dir/ALL_RESULTS.md" 2>/dev/null || echo 0)
    compiles=$(grep -c '| COMPILES |' "$dir/ALL_RESULTS.md" 2>/dev/null || echo 0)
    wrote=$(grep -c '| WROTE |' "$dir/ALL_RESULTS.md" 2>/dev/null || echo 0)
    fail=$(grep -c '| FAIL |' "$dir/ALL_RESULTS.md" 2>/dev/null || echo 0)
    rate=$(awk "BEGIN {printf \"%.0f\", ($green/$total)*100}")
    printf '| %s | %d | %d | %d | %d | %d | %d | %d | %s%% |\n' \
      "$name" "$total" "$green" "$partial" "$unverified" "$compiles" "$wrote" "$fail" "$rate" \
      >> "$SUMMARY_FILE"
  done

  # Per-project stability tracking
  cat >> "$SUMMARY_FILE" << 'EOF'

## Per-Project Stability (last 5 runs)

| Project | Run1 | Run2 | Run3 | Run4 | Run5 | Consistency |
|---------|------|------|------|------|------|-------------|
EOF

  # Collect all project names seen
  local projects
  projects="$(grep '^| C' "$LONG_RUN_DIR"/system_test_8hr_v*/ALL_RESULTS.md 2>/dev/null \
    | awk -F'|' '{gsub(/^ +| +$/,"",$4); print $4}' | sort -u)"

  for proj in $projects; do
    local row="| $proj "
    local green_count=0 total_count=0
    for dir in $(ls -t "$LONG_RUN_DIR"/system_test_8hr_v* -d 2>/dev/null | head -5); do
      [[ -f "$dir/ALL_RESULTS.md" ]] || continue
      local st
      st="$(grep "| $proj |" "$dir/ALL_RESULTS.md" 2>/dev/null | tail -1 | awk -F'|' '{gsub(/^ +| +$/,"",$5); print $5}')"
      if [[ -n "$st" ]]; then
        row+="| $st "
        total_count=$((total_count + 1))
        [[ "$st" == "GREEN" ]] && green_count=$((green_count + 1))
      else
        row+="| - "
      fi
    done
    # Pad to 5 columns
    local cols=$total_count
    while (( cols < 5 )); do
      row+="| - "
      cols=$((cols + 1))
    done
    # Consistency rating
    if (( total_count == 0 )); then
      row+="| new |"
    elif (( green_count == total_count )); then
      row+="| STABLE |"
    elif (( green_count == 0 )); then
      row+="| BROKEN |"
    else
      row+="| FLAKY |"
    fi
    echo "$row" >> "$SUMMARY_FILE"
  done

  log "Report written to $SUMMARY_FILE"
}

# ── Cleanup on exit ──
cleanup() {
  log "Monitor stopping ($(elapsed_human) elapsed, $CYCLE cycles)."
  generate_report
  exit 0
}
trap cleanup INT TERM

# ═══════════════════════════════════════
# MAIN LOOP
# ═══════════════════════════════════════
log "════════════════════════════════════════"
log "  4-DAY CONTINUOUS TEST LOOP"
log "  Duration: ${DURATION_DAYS} days"
log "  Endpoint: $ENDPOINT"
log "  Model: $MODEL"
log "  Log: $LOG_FILE"
log "════════════════════════════════════════"

while (( $(time_remaining) > 3600 )); do
  # 1. Wait for any running test
  running_pid="$(find_running_test)" || true
  if [[ -n "$running_pid" ]]; then
    wait_for_test "$running_pid"
    generate_report
  fi

  # 2. Check time budget
  if (( $(time_remaining) < 7200 )); then
    log "Less than 2h remaining — stopping."
    break
  fi

  # 3. Wait for endpoint
  wait_for_endpoint || break

  # 4. Rebuild if source changed
  maybe_rebuild

  # 5. Pick latest test script
  script="$(latest_test_script)"
  if [[ -z "$script" ]]; then
    log "ERROR: No test script found in $LONG_RUN_DIR"
    break
  fi

  # 6. Preflight
  if ! preflight "$script"; then
    log "Preflight failed — waiting 30 min before retry."
    sleep 1800
    continue
  fi

  # 7. Launch
  test_pid="$(launch_test "$script")" || break

  # 8. Wait for it
  wait_for_test "$test_pid"

  # 9. Report
  generate_report
  log "Cycle $CYCLE complete. $(elapsed_human) elapsed, $(time_remaining)s remaining."
done

log "════════════════════════════════════════"
log "  4-DAY LOOP COMPLETE: $CYCLE cycles"
log "  $(elapsed_human) elapsed"
log "════════════════════════════════════════"
generate_report
