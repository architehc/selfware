#!/usr/bin/env bash
# 8-hour system test v6: UNVERIFIED/TAMPERED_TESTS result classes, adaptive retry demotion, lower default iters.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LONG_RUN_DIR="$ROOT_DIR/long_run_tests"
SELFWARE="${SELFWARE:-$ROOT_DIR/target/release/selfware}"
ENDPOINT="${ENDPOINT:-https://crazyshit.ngrok.io/v1}"
MODEL="${MODEL:-txn545/Qwen3.5-122B-A10B-NVFP4}"
TEMPLATES="${TEMPLATES:-$ROOT_DIR/system_tests/projecte2e/templates}"
DURATION_HOURS="${DURATION_HOURS:-8}"
DURATION_SECS="${DURATION_SECS:-$((DURATION_HOURS * 3600))}"
TIMEOUT_PER_PROJECT="${TIMEOUT:-1200}"
MIN_PROJECT_WINDOW_SECS="${MIN_PROJECT_WINDOW_SECS:-120}"
MAX_ITERS="${MAX_ITERS:-80}"
STATUS_INTERVAL_SECS="${STATUS_INTERVAL_SECS:-60}"
RUN_ID="${RUN_ID_OVERRIDE:-system_test_8hr_v6_$(date +%Y%m%d_%H%M%S)}"
MASTER_DIR="${RESULTS_DIR_OVERRIDE:-$LONG_RUN_DIR/$RUN_ID}"

mkdir -p "$MASTER_DIR"

MASTER_LOG="$MASTER_DIR/master.log"
STATE_FILE="$MASTER_DIR/current_state.env"
STATUS_FILE="$MASTER_DIR/STATUS.md"
EVENT_LOG="$MASTER_DIR/events.log"
PID_FILE="$MASTER_DIR/run.pid"
MONITOR_PID_FILE="$MASTER_DIR/monitor.pid"
FINAL_REPORT="$MASTER_DIR/FINAL_REPORT.md"

touch "$EVENT_LOG"
exec > >(tee -a "$MASTER_LOG") 2>&1

START_TIME="$(date +%s)"
DEADLINE_TS="$((START_TIME + DURATION_SECS))"
CURRENT_CYCLE=0
ACTIVE_ROUND=0
ACTIVE_ROUND_LABEL="idle"
ACTIVE_ROUND_DIR=""
ACTIVE_PROJECT=""
ACTIVE_LOG=""
LAST_EVENT="initialized"
STOP_REQUESTED=0
MONITOR_PID=""
RUNNING=1

# ── Project history for adaptive retry ──
PROJECT_HISTORY="$MASTER_DIR/project_history.tsv"
printf 'project\tcycle\tstatus\treason\n' > "$PROJECT_HISTORY"

# Record a project outcome for adaptive retry decisions.
record_outcome() {
  local proj="$1" cycle="$2" status="$3" reason="$4"
  printf '%s\t%s\t%s\t%s\n' "$proj" "$cycle" "$status" "$reason" >> "$PROJECT_HISTORY"
}

# Count how many times a project has had a given status across cycles.
count_prior_status() {
  local proj="$1" status="$2"
  awk -F'\t' -v p="$proj" -v s="$status" '$1==p && $3==s {c++} END {print c+0}' "$PROJECT_HISTORY"
}

# Should we skip this project? Quarantine after 2 identical non-GREEN outcomes.
should_skip_project() {
  local proj="$1"
  local prev_fails
  prev_fails="$(awk -F'\t' -v p="$proj" '$1==p && $3!="GREEN" {c++} END {print c+0}' "$PROJECT_HISTORY")"
  if (( prev_fails >= 2 )); then
    append_event "QUARANTINE: skipping $proj after $prev_fails prior non-GREEN outcomes"
    echo "1"
  else
    echo "0"
  fi
}

printf '%s\n' "$$" > "$PID_FILE"

strip_ansi() {
  sed -r 's/\x1B\[[0-9;]*[A-Za-z]//g'
}

elapsed_human() {
  local delta
  delta=$(( $(date +%s) - START_TIME ))
  printf "%dh%02dm" $(( delta / 3600 )) $(( (delta % 3600) / 60 ))
}

seconds_to_human() {
  local total="$1"
  printf "%dh%02dm" $(( total / 3600 )) $(( (total % 3600) / 60 ))
}

time_remaining() {
  local remaining
  remaining=$(( DEADLINE_TS - $(date +%s) ))
  if (( remaining > 0 )); then
    echo "$remaining"
  else
    echo 0
  fi
}

project_timeout() {
  local remaining
  remaining="$(time_remaining)"
  if (( remaining <= 0 )); then
    echo 0
  elif (( remaining < TIMEOUT_PER_PROJECT )); then
    echo "$remaining"
  else
    echo "$TIMEOUT_PER_PROJECT"
  fi
}

count_status() {
  local status="$1"
  grep -c "| $status |" "$MASTER_DIR/ALL_RESULTS.md" 2>/dev/null || true
}

count_total_rows() {
  grep -Ec '^\| C[0-9]+ \| R[0-9]+ \|' "$MASTER_DIR/ALL_RESULTS.md" 2>/dev/null || true
}

write_state() {
  {
    printf 'RUN_ID=%q\n' "$RUN_ID"
    printf 'MASTER_DIR=%q\n' "$MASTER_DIR"
    printf 'START_TIME=%q\n' "$START_TIME"
    printf 'DEADLINE_TS=%q\n' "$DEADLINE_TS"
    printf 'CURRENT_CYCLE=%q\n' "$CURRENT_CYCLE"
    printf 'ACTIVE_ROUND=%q\n' "$ACTIVE_ROUND"
    printf 'ACTIVE_ROUND_LABEL=%q\n' "$ACTIVE_ROUND_LABEL"
    printf 'ACTIVE_ROUND_DIR=%q\n' "$ACTIVE_ROUND_DIR"
    printf 'ACTIVE_PROJECT=%q\n' "$ACTIVE_PROJECT"
    printf 'ACTIVE_LOG=%q\n' "$ACTIVE_LOG"
    printf 'LAST_EVENT=%q\n' "$LAST_EVENT"
    printf 'STOP_REQUESTED=%q\n' "$STOP_REQUESTED"
    printf 'RUNNING=%q\n' "$RUNNING"
  } > "$STATE_FILE"
}

snapshot_status() {
  local now remaining tmp_status active_elapsed
  now="$(date +%s)"
  remaining="$(time_remaining)"
  if [[ -n "$ACTIVE_PROJECT" && -n "$ACTIVE_LOG" && -f "$ACTIVE_LOG" ]]; then
    active_elapsed=$(( now - $(stat -c %Y "$ACTIVE_LOG" 2>/dev/null || echo "$now") ))
  else
    active_elapsed=0
  fi

  tmp_status="$(mktemp "${STATUS_FILE}.XXXXXX")"
  {
    printf '# 8-Hour System Test v4 Status\n\n'
    printf -- '- Run id: `%s`\n' "$RUN_ID"
    printf -- '- Results dir: `%s`\n' "$MASTER_DIR"
    printf -- '- Started: `%s`\n' "$(date -d "@$START_TIME" '+%Y-%m-%d %H:%M:%S %Z')"
    printf -- '- Deadline: `%s`\n' "$(date -d "@$DEADLINE_TS" '+%Y-%m-%d %H:%M:%S %Z')"
    printf -- '- Configured duration: `%s`\n' "$(seconds_to_human "$DURATION_SECS")"
    printf -- '- Elapsed: `%s`\n' "$(elapsed_human)"
    printf -- '- Seconds remaining: `%s`\n' "$remaining"
    printf -- '- Cycle: `%s`\n' "$CURRENT_CYCLE"
    printf -- '- Round: `%s`\n' "$ACTIVE_ROUND"
    printf -- '- Round label: `%s`\n' "$ACTIVE_ROUND_LABEL"
    printf -- '- Active project: `%s`\n' "${ACTIVE_PROJECT:-idle}"
    printf -- '- Active round dir: `%s`\n' "${ACTIVE_ROUND_DIR:-none}"
    printf -- '- Active log: `%s`\n' "${ACTIVE_LOG:-none}"
    printf -- '- Active log age seconds: `%s`\n' "$active_elapsed"
    printf -- '- Last event: `%s`\n' "$LAST_EVENT"
    printf -- '- Running: `%s`\n' "$RUNNING"
    printf '\n'

    if [[ -f "$MASTER_DIR/ALL_RESULTS.md" ]]; then
      printf '## Scoreboard\n\n'
      printf -- '- Total rows: `%s`\n' "$(count_total_rows)"
      printf -- '- GREEN: `%s`\n' "$(count_status GREEN)"
      printf -- '- PARTIAL: `%s`\n' "$(count_status PARTIAL)"
      printf -- '- UNVERIFIED: `%s`\n' "$(count_status UNVERIFIED)"
      printf -- '- TAMPERED_TESTS: `%s`\n' "$(count_status TAMPERED_TESTS)"
      printf -- '- COMPILES: `%s`\n' "$(count_status COMPILES)"
      printf -- '- WROTE: `%s`\n' "$(count_status WROTE)"
      printf -- '- FAIL: `%s`\n' "$(count_status FAIL)"
      printf -- '- QUARANTINED: `%s`\n' "$(count_status QUARANTINED)"
      printf '\n'
      printf '## Results So Far\n\n'
      sed -n '1,120p' "$MASTER_DIR/ALL_RESULTS.md"
      printf '\n'
    fi

    if [[ -n "$ACTIVE_LOG" && -f "$ACTIVE_LOG" ]]; then
      printf '## Active Log Tail\n\n'
      tail -10 "$ACTIVE_LOG" 2>/dev/null | strip_ansi
      printf '\n'
    fi

    if [[ -f "$EVENT_LOG" ]]; then
      printf '## Recent Events\n\n'
      tail -20 "$EVENT_LOG"
      printf '\n'
    fi
  } > "$tmp_status"

  mv "$tmp_status" "$STATUS_FILE"
}

append_event() {
  LAST_EVENT="$1"
  printf '[%s] %s\n' "$(date '+%Y-%m-%d %H:%M:%S %Z')" "$1" | tee -a "$EVENT_LOG"
  write_state
  snapshot_status
}

monitor_loop() {
  while true; do
    if [[ -f "$STATE_FILE" ]]; then
      # shellcheck disable=SC1090
      source "$STATE_FILE"
    fi
    snapshot_status
    if (( $(date +%s) >= DEADLINE_TS )); then
      break
    fi
    sleep "$STATUS_INTERVAL_SECS"
  done
}

cleanup_on_exit() {
  RUNNING=0
  ACTIVE_PROJECT=""
  ACTIVE_ROUND_LABEL="idle"
  write_state
  snapshot_status
  if [[ -n "${MONITOR_PID:-}" ]]; then
    kill "$MONITOR_PID" 2>/dev/null || true
  fi
}

shutdown_on_signal() {
  STOP_REQUESTED=1
  append_event "Received stop signal"
  exit 130
}

trap cleanup_on_exit EXIT
trap shutdown_on_signal INT TERM

git_bootstrap() {
  local dir="$1"
  (
    cd "$dir"
    git init -q
    git add -A
    git commit -q -m init >/dev/null 2>&1 || true
  )
}

make_config() {
  local dir="$1"
  cat > "$dir/selfware.toml" << EOF
endpoint = "$ENDPOINT"
model = "$MODEL"
max_tokens = 16384
context_length = 262144
temperature = 0.6

[safety]
allowed_paths = ["./**", "/tmp/**"]

[agent]
max_iterations = $MAX_ITERS
step_timeout_secs = 600
native_function_calling = false
streaming = true
min_completion_steps = 10
require_verification_before_completion = true

[continuous_work]
enabled = true
checkpoint_interval_tools = 5
auto_recovery = true
max_recovery_attempts = 5

[extra_body]
chat_template_kwargs = { enable_thinking = false }

[retry]
max_retries = 5
base_delay_ms = 2000
max_delay_ms = 60000
EOF
}

setup_greenfield() {
  local name="$1"
  local dir="$2"
  mkdir -p "$dir/src"
  cat > "$dir/Cargo.toml" << EOF
[package]
name = "$name"
version = "0.1.0"
edition = "2021"
EOF
  printf '%s\n' '// implement here' > "$dir/src/lib.rs"
  make_config "$dir"
  git_bootstrap "$dir"
}

setup_template() {
  local template="$1"
  local dir="$2"
  cp -r "$TEMPLATES/$template" "$dir"
  rm -rf "$dir/target" "$dir/Cargo.lock" "$dir/node_modules"
  make_config "$dir"
  # Save checksums of test files for tamper detection
  find "$dir/tests" "$dir" -maxdepth 1 \( -name '*_test.rs' -o -name '*_tests.rs' -o -name 'test_*.py' -o -name '*_test.go' \) -type f 2>/dev/null \
    | sort | xargs md5sum 2>/dev/null > "$dir/.test_checksums" || true
  # Count expected tests so we can detect UNVERIFIED
  local expected_tests=0
  if [[ -d "$dir/tests" ]] || find "$dir" -maxdepth 1 -name '*_test*' -o -name 'test_*' 2>/dev/null | grep -q .; then
    expected_tests=1
  fi
  printf '%s\n' "$expected_tests" > "$dir/.has_expected_tests"
  git_bootstrap "$dir"
}

setup_python() {
  local dir="$1"
  mkdir -p "$dir"
  make_config "$dir"
  git_bootstrap "$dir"
}

setup_go() {
  local dir="$1"
  local module="$2"
  mkdir -p "$dir"
  cat > "$dir/go.mod" << EOF
module $module

go 1.21
EOF
  make_config "$dir"
  git_bootstrap "$dir"
}

extract_reason() {
  local log="$1"
  local exit_code="$2"
  local outcome reason

  outcome="$(grep -o 'Outcome: [a-z_]*' "$log" 2>/dev/null | tail -1 | awk '{print $2}' || true)"
  reason="$(grep -oE '\[[a-z_]+\]' "$log" 2>/dev/null | tail -1 | tr -d '[]' || true)"

  if [[ -z "$reason" && -n "$outcome" ]]; then
    reason="$outcome"
  fi
  if [[ -z "$reason" && "$exit_code" == "124" ]]; then
    reason="timeout"
  fi
  if [[ -z "$reason" ]] && grep -q "Max iterations exceeded" "$log" 2>/dev/null; then
    reason="max_iterations"
  fi
  if [[ -z "$reason" ]] && grep -q "verification_missing" "$log" 2>/dev/null; then
    reason="verification_missing"
  fi
  if [[ -z "$reason" ]]; then
    reason="none"
  fi

  printf '%s' "$reason"
}

count_matching_lines() {
  local root="$1"
  shift
  local total=0
  local file
  while IFS= read -r file; do
    [[ -f "$file" ]] || continue
    total=$(( total + $(wc -l < "$file") ))
  done < <(find "$root" "$@" -type f 2>/dev/null)
  printf '%s' "$total"
}

run_project() {
  local name="$1"
  local workdir="$2"
  local task="$3"
  local round="$4"
  local log="$ROUND_DIR/${name}.log"
  local timeout_secs
  local exit_code
  local proj_start
  local proj_end
  local dur
  local steps
  local reason
  local compile="no"
  local test_result="-"
  local passed=0
  local failed_count=0
  local src_lines=0
  local status="FAIL"

  if (( STOP_REQUESTED == 1 )); then
    return 1
  fi

  # Adaptive retry: skip quarantined projects
  if [[ "$(should_skip_project "$name")" == "1" ]]; then
    printf "  [%s] QUARANTINED — skipping\n" "$name"
    printf "| C%d | R%d | %s | %s | %d | %s | %s | %s | %s | %s |\n" \
      "$CURRENT_CYCLE" "$round" "$name" "QUARANTINED" "0" "0" "0" "no" "-" "quarantined" \
      >> "$MASTER_DIR/ALL_RESULTS.md"
    return 0
  fi

  if (( $(time_remaining) < MIN_PROJECT_WINDOW_SECS )); then
    STOP_REQUESTED=1
    append_event "Stopping before $name because remaining budget is below ${MIN_PROJECT_WINDOW_SECS}s"
    return 1
  fi

  timeout_secs="$(project_timeout)"
  if (( timeout_secs <= 0 )); then
    STOP_REQUESTED=1
    append_event "Stopping before $name because no time remains"
    return 1
  fi

  ACTIVE_PROJECT="$name"
  ACTIVE_LOG="$log"
  write_state
  snapshot_status
  append_event "Starting cycle $CURRENT_CYCLE round $round project $name timeout=${timeout_secs}s"

  echo -n "  [$name] "
  proj_start="$(date +%s)"

  if timeout "$timeout_secs" "$SELFWARE" \
    -c "$workdir/selfware.toml" \
    -C "$workdir" \
    --yolo \
    --ascii \
    --no-color \
    -p "$task" \
    > "$log" 2>&1; then
    exit_code=0
  else
    exit_code=$?
  fi

  proj_end="$(date +%s)"
  dur=$(( proj_end - proj_start ))
  steps="$(grep -c 'Step.*Executing' "$log" 2>/dev/null || true)"
  reason="$(extract_reason "$log" "$exit_code")"

  if [[ -f "$workdir/Cargo.toml" ]]; then
    src_lines="$(count_matching_lines "$workdir/src" -name '*.rs')"
    local cargo_out test_out
    if cargo_out="$(cd "$workdir" && cargo check 2>&1)"; then
      compile="YES"
      if test_out="$(cd "$workdir" && cargo test 2>&1)"; then
        :
      else
        :
      fi
      passed="$(printf '%s\n' "$test_out" | grep -o '[0-9]\+ passed' | awk '{s+=$1} END {print s+0}')"
      failed_count="$(printf '%s\n' "$test_out" | grep -o '[0-9]\+ failed' | awk '{s+=$1} END {print s+0}')"
      test_result="${passed}p/${failed_count}f"
    fi
  fi

  local py_test_file
  py_test_file="$(find "$workdir" -maxdepth 1 -type f -name 'test_*.py' 2>/dev/null | head -1 || true)"
  if [[ -n "$py_test_file" ]]; then
    src_lines="$(count_matching_lines "$workdir" -maxdepth 1 -name '*.py' ! -name 'test_*')"
    local py_out
    if py_out="$(cd "$workdir" && python3 -m pytest -v 2>&1)"; then
      compile="YES"
    else
      :
    fi
    if printf '%s\n' "$py_out" | grep -Eq '([0-9]+ passed|[0-9]+ failed|collected [0-9]+ items)'; then
      compile="YES"
      passed="$(printf '%s\n' "$py_out" | grep -o '[0-9]\+ passed' | awk '{print $1}' | tail -1)"
      failed_count="$(printf '%s\n' "$py_out" | grep -o '[0-9]\+ failed' | awk '{print $1}' | tail -1)"
      passed="${passed:-0}"
      failed_count="${failed_count:-0}"
      test_result="${passed}p/${failed_count}f"
    fi
  fi

  if [[ -f "$workdir/go.mod" ]]; then
    src_lines="$(count_matching_lines "$workdir" -maxdepth 1 -name '*.go' ! -name '*_test.go')"
    export XDG_RUNTIME_DIR=/tmp/xdg-run-selfware
    mkdir -p "$XDG_RUNTIME_DIR" 2>/dev/null || true
    local go_out
    if go_out="$(cd "$workdir" && go test -v ./... 2>&1)"; then
      compile="YES"
    else
      :
    fi
    if printf '%s\n' "$go_out" | grep -q '^ok'; then
      compile="YES"
      passed="$(printf '%s\n' "$go_out" | grep -cF -- '--- PASS' || true)"
      failed_count="$(printf '%s\n' "$go_out" | grep -cF -- '--- FAIL' || true)"
      test_result="${passed}p/${failed_count}f"
    elif printf '%s\n' "$go_out" | grep -qF -- '--- FAIL'; then
      compile="YES"
      passed="$(printf '%s\n' "$go_out" | grep -cF -- '--- PASS' || true)"
      failed_count="$(printf '%s\n' "$go_out" | grep -cF -- '--- FAIL' || true)"
      test_result="${passed}p/${failed_count}f"
    fi
  fi

  # ── Tamper detection: check if test files were modified ──
  local tampered=0
  if [[ -f "$workdir/.test_checksums" ]]; then
    local current_checksums
    current_checksums="$(cd "$workdir" && md5sum $(awk '{print $2}' .test_checksums 2>/dev/null) 2>/dev/null || true)"
    local original_checksums
    original_checksums="$(cat "$workdir/.test_checksums")"
    if [[ -n "$original_checksums" && "$current_checksums" != "$original_checksums" ]]; then
      tampered=1
    fi
  fi

  # ── Template test expectation: UNVERIFIED if template has tests but 0 found ──
  local has_expected_tests=0
  if [[ -f "$workdir/.has_expected_tests" ]]; then
    has_expected_tests="$(cat "$workdir/.has_expected_tests")"
  fi

  if [[ "$tampered" -eq 1 ]]; then
    status="TAMPERED_TESTS"
  elif [[ "$compile" == "YES" && "$passed" -gt 0 && "$failed_count" -eq 0 ]]; then
    status="GREEN"
  elif [[ "$compile" == "YES" && "$passed" -gt 0 ]]; then
    status="PARTIAL"
  elif [[ "$compile" == "YES" && "$has_expected_tests" -eq 1 && "$passed" -eq 0 ]]; then
    status="UNVERIFIED"
  elif [[ "$compile" == "YES" ]]; then
    status="COMPILES"
  elif [[ "$src_lines" -gt 2 ]]; then
    status="WROTE"
  fi

  # Record for adaptive retry
  record_outcome "$name" "$CURRENT_CYCLE" "$status" "$reason"

  printf "%s | %ds steps=%s src=%sL comp=%s tests=%s reason=%s\n" \
    "$status" "$dur" "$steps" "$src_lines" "$compile" "$test_result" "$reason"

  printf "| %s | %s | %d | %s | %s | %s | %s | %s |\n" \
    "$name" "$status" "$dur" "$steps" "$src_lines" "$compile" "$test_result" "$reason" \
    >> "$ROUND_DIR/SUMMARY.md"

  printf "| C%d | R%d | %s | %s | %d | %s | %s | %s | %s | %s |\n" \
    "$CURRENT_CYCLE" "$round" "$name" "$status" "$dur" "$steps" "$src_lines" "$compile" "$test_result" "$reason" \
    >> "$MASTER_DIR/ALL_RESULTS.md"

  append_event "Finished cycle $CURRENT_CYCLE round $round project $name status=$status reason=$reason"
  ACTIVE_PROJECT=""
  write_state
  snapshot_status
}

start_round() {
  local round="$1"
  local slug="$2"
  local label="$3"
  local summary_title="$4"

  ACTIVE_ROUND="$round"
  ACTIVE_ROUND_LABEL="$label"
  ACTIVE_ROUND_DIR="$MASTER_DIR/cycle_$(printf '%02d' "$CURRENT_CYCLE")_round_$(printf '%02d' "$round")_${slug}"
  ROUND_DIR="$ACTIVE_ROUND_DIR"

  mkdir -p "$ROUND_DIR"
  cat > "$ROUND_DIR/SUMMARY.md" << EOF
# Cycle $CURRENT_CYCLE Round $round: $summary_title

| Project | Status | Time(s) | Steps | SrcLines | Compiles | Tests | Reason |
|---------|--------|---------|-------|----------|----------|-------|--------|
EOF

  append_event "Starting cycle $CURRENT_CYCLE round $round: $label"
  echo "== C${CURRENT_CYCLE} R${round}: $label ($(elapsed_human)) =="
}

finish_round() {
  append_event "Finished cycle $CURRENT_CYCLE round $ACTIVE_ROUND"
  echo "== Cycle $CURRENT_CYCLE Round $ACTIVE_ROUND complete ($(elapsed_human)) =="
  echo
}

run_round_1() {
  start_round 1 "retry_greenfield" "Retry Greenfield" "Retry Greenfield (with early-quit fix)"

  P="expression_eval"; W="$ROUND_DIR/$P"
  setup_greenfield "expression-eval" "$W"
  run_project "$P" "$W" \
    "Build an arithmetic expression evaluator in src/lib.rs using recursive descent parsing. Support: integers, +, -, *, /, parentheses, unary minus. pub fn eval(expr: &str) -> Result<f64, String>. Write 12 unit tests including: basic ops, precedence (2+3*4=14), parens ((2+3)*4=20), nested parens, unary minus, division by zero error, empty input error. Run cargo test." 1 || return 0

  P="lru_cache"; W="$ROUND_DIR/$P"
  setup_greenfield "lru-cache" "$W"
  run_project "$P" "$W" \
    "Build an LRU cache in src/lib.rs: pub struct LruCache<K, V> with new(capacity), get(&mut self, key: &K) -> Option<&V>, put(&mut self, key: K, value: V), len(), contains_key(&K). Use HashMap + VecDeque. Evict LRU on capacity overflow. get() updates recency. Write 10 unit tests. Run cargo test." 1 || return 0

  P="csv_parser"; W="$ROUND_DIR/$P"
  setup_greenfield "csv-parser" "$W"
  run_project "$P" "$W" \
    "Build a CSV parser in src/lib.rs: pub fn parse_csv(input: &str) -> Vec<Vec<String>>. Handle commas, quoted fields, escaped quotes, newlines inside quotes, empty fields. Also pub fn to_csv(rows: &[Vec<&str>]) -> String. Write 10 unit tests. Run cargo test." 1 || return 0

  P="matrix_ops"; W="$ROUND_DIR/$P"
  setup_greenfield "matrix-ops" "$W"
  run_project "$P" "$W" \
    "Build a matrix library in src/lib.rs: pub struct Matrix with new(rows, cols, data: Vec<f64>), add, multiply, transpose, determinant (2x2 and 3x3), identity(n). Write 10 unit tests. Run cargo test." 1 || return 0

  P="html_report"; W="$ROUND_DIR/$P"
  setup_greenfield "html-report" "$W"
  run_project "$P" "$W" \
    "Create an HTML report generator in src/lib.rs. ReportBuilder with new(title), add_heading(text, level), add_paragraph(text), add_table(headers, rows), add_code_block(code, lang), build() -> String (valid HTML with CSS). Write 8 unit tests. Run cargo test." 1 || return 0

  finish_round
}

run_round_2() {
  start_round 2 "templates_expanded" "More Templates" "More Templates"

  P="viz_histogram"; W="$ROUND_DIR/$P"
  setup_template "viz_histogram" "$W"
  run_project "$P" "$W" \
    "Read all source files in src/ and the test file in tests/. This is a histogram renderer with color support. Fix all bugs so every test passes. Do NOT change test files. Run cargo test." 2 || return 0

  P="viz_ascii_table"; W="$ROUND_DIR/$P"
  setup_template "viz_ascii_table" "$W"
  run_project "$P" "$W" \
    "Read src/lib.rs and tests/table_tests.rs carefully. The Table struct has 3 bugs: (1) column width off-by-one in saturating_sub(1), (2) horizontal lines use '-' instead of box char '─', (3) Right alignment uses left-align format. Fix ONLY these 3 bugs. Do NOT restructure the code or change function signatures. Do NOT change tests. Run cargo test." 2 || return 0

  P="viz_maze_gen"; W="$ROUND_DIR/$P"
  setup_template "viz_maze_gen" "$W"
  run_project "$P" "$W" \
    "Read all source files in src/ and tests/. This is a maze generator with grid, generator, and ASCII renderer. Fix all incomplete or buggy functions. Do NOT change test files. Run cargo test." 2 || return 0

  P="viz_svg_chart"; W="$ROUND_DIR/$P"
  setup_template "viz_svg_chart" "$W"
  run_project "$P" "$W" \
    "Read all source files and tests. This is an SVG chart generator. Fix all bugs so tests pass. Do NOT change test files. Run cargo test." 2 || return 0

  finish_round
}

run_round_3() {
  start_round 3 "multilang_retry" "Multi-Language Retry" "Multi-Language Retry"

  P="python_json_tool"; W="$ROUND_DIR/$P"
  setup_python "$W"
  run_project "$P" "$W" \
    "Create json_tool.py with: flatten_json(nested_dict, sep='.') -> dict, unflatten_json(flat_dict, sep='.') -> dict, diff_json(a, b) -> dict with 'added', 'removed', 'changed' keys. Create test_json_tool.py with 10 pytest tests. Run python3 -m pytest test_json_tool.py -v." 3 || return 0

  P="python_text_stats"; W="$ROUND_DIR/$P"
  setup_python "$W"
  run_project "$P" "$W" \
    "Create text_stats.py with: word_count(text) -> int, char_frequency(text) -> dict, sentence_count(text) -> int, average_word_length(text) -> float, most_common_words(text, n=5) -> list[tuple]. Create test_text_stats.py with 8 pytest tests. Run python3 -m pytest -v." 3 || return 0

  P="go_calculator"; W="$ROUND_DIR/$P"
  setup_go "$W" "calculator"
  run_project "$P" "$W" \
    "Create calculator.go with package calculator and functions: Add(a, b float64) float64, Subtract(a, b float64) float64, Multiply(a, b float64) float64, Divide(a, b float64) (float64, error). Create calculator_test.go with 8 tests. Run go test -v ./..." 3 || return 0

  P="go_stack"; W="$ROUND_DIR/$P"
  setup_go "$W" "stack"
  run_project "$P" "$W" \
    "Create stack.go with package stack and a generic Stack[T any] type: New[T]() *Stack[T], Push(val T), Pop() (T, bool), Peek() (T, bool), Len() int, IsEmpty() bool. Create stack_test.go with 8 tests. Run go test -v ./..." 3 || return 0

  finish_round
}

run_round_4() {
  start_round 4 "hard_greenfield" "Hard Greenfield" "Hard Greenfield"

  P="trie"; W="$ROUND_DIR/$P"
  setup_greenfield "trie" "$W"
  run_project "$P" "$W" \
    "Build a trie (prefix tree) in src/lib.rs: pub struct Trie with new(), insert(word: &str), contains(word: &str) -> bool, starts_with(prefix: &str) -> bool, words_with_prefix(prefix: &str) -> Vec<String>, remove(word: &str) -> bool. Write 10 unit tests. Run cargo test." 4 || return 0

  P="roman"; W="$ROUND_DIR/$P"
  setup_greenfield "roman" "$W"
  run_project "$P" "$W" \
    "Create a Roman numeral converter in src/lib.rs. Two functions: pub fn to_roman(n: u32) -> String and pub fn from_roman(s: &str) -> Option<u32>. Use the standard subtractive Roman numeral rules. Write 10 unit tests. Run cargo test." 4 || return 0

  P="json_patch"; W="$ROUND_DIR/$P"
  setup_greenfield "json-patch" "$W"
  cat >> "$W/Cargo.toml" << 'EOF'

[dependencies]
serde_json = "1"
EOF
  (
    cd "$W"
    git add -A
    git commit -q -m "add serde_json dep" >/dev/null 2>&1 || true
  )
  run_project "$P" "$W" \
    "Build a JSON merge patch (RFC 7396) in src/lib.rs using serde_json::Value. pub fn merge(base: &Value, patch: &Value) -> Value. Recursively merge objects, null in patch removes keys, and non-object patch replaces entirely. Write 10 unit tests. Run cargo test." 4 || return 0

  P="ring_buffer"; W="$ROUND_DIR/$P"
  setup_greenfield "ring-buffer" "$W"
  run_project "$P" "$W" \
    "Build a ring buffer in src/lib.rs: pub struct RingBuffer<T> with new(capacity), push(val: T), pop() -> Option<T>, peek() -> Option<&T>, len(), is_full(), is_empty(), iter() -> impl Iterator. Use a fixed-size Vec with head and tail indices. Write 10 unit tests. Run cargo test." 4 || return 0

  finish_round
}

run_round_5() {
  start_round 5 "template_hard" "Hard Templates" "Hard Templates"

  P="hard_event_bus"; W="$ROUND_DIR/$P"
  setup_template "hard_event_bus" "$W"
  run_project "$P" "$W" \
    "Read all source files in src/ and tests/. This is a pub-sub event bus with topic filtering. Fix all bugs so every test passes. Do NOT change test files. Run cargo test." 5 || return 0

  P="hard_scheduler"; W="$ROUND_DIR/$P"
  setup_template "hard_scheduler" "$W"
  run_project "$P" "$W" \
    "Read all source files in src/ and tests/. This is a task scheduler with priority and duration. Fix all bugs so every test passes. Do NOT change test files. Run cargo test." 5 || return 0

  P="easy_calculator"; W="$ROUND_DIR/$P"
  setup_template "easy_calculator" "$W"
  run_project "$P" "$W" \
    "Read src/lib.rs and tests/calc_tests.rs. The calculator has bugs. Fix only the bugs. Do NOT rewrite or restructure. Do NOT change function signatures or the test file. Run cargo test until all tests pass." 5 || return 0

  P="easy_string_ops"; W="$ROUND_DIR/$P"
  setup_template "easy_string_ops" "$W"
  run_project "$P" "$W" \
    "Read all files in src/ and tests/. Fix all bugs in the string operations code. Do NOT change test files or function signatures. Run cargo test until all pass." 5 || return 0

  finish_round
}

run_round_6() {
  start_round 6 "stress_ds" "Data Structures Stress" "Data Structures Stress"

  P="hashmap"; W="$ROUND_DIR/$P"
  setup_greenfield "hashmap" "$W"
  run_project "$P" "$W" \
    "Build a simple hash map in src/lib.rs: pub struct SimpleHashMap<V> with new(), insert(key: &str, value: V), get(key: &str) -> Option<&V>, remove(key: &str) -> Option<V>, len(), contains_key(key: &str). Use separate chaining. Write 10 unit tests. Run cargo test." 6 || return 0

  P="binary_search"; W="$ROUND_DIR/$P"
  setup_greenfield "binary-search" "$W"
  run_project "$P" "$W" \
    "Build a sorted list with binary search in src/lib.rs: pub struct SortedList<T: Ord> with new(), insert(val: T), contains(val: &T) -> bool, remove(val: &T) -> bool, len(), get(index: usize) -> Option<&T>, range(from: &T, to: &T) -> Vec<&T>. Keep internal Vec sorted. Write 10 unit tests. Run cargo test." 6 || return 0

  P="state_machine"; W="$ROUND_DIR/$P"
  setup_greenfield "state-machine" "$W"
  run_project "$P" "$W" \
    "Build a finite state machine in src/lib.rs: pub struct StateMachine with new(initial_state: &str), add_transition(from: &str, event: &str, to: &str), handle_event(&mut self, event: &str) -> Result<&str, String>, current_state() -> &str, valid_events() -> Vec<String>. Write 8 unit tests including invalid transitions. Run cargo test." 6 || return 0

  P="tokenizer"; W="$ROUND_DIR/$P"
  setup_greenfield "tokenizer" "$W"
  run_project "$P" "$W" \
    "Build a tokenizer in src/lib.rs for arithmetic expressions. pub enum Token { Number(f64), Plus, Minus, Star, Slash, LParen, RParen }. pub fn tokenize(input: &str) -> Result<Vec<Token>, String>. Handle multi-digit numbers, decimals, and whitespace. Write 10 unit tests. Run cargo test." 6 || return 0

  finish_round
}

run_round_7() {
  start_round 7 "reliability" "Reliability Check" "Reliability Check"

  P="calculator_r2"; W="$ROUND_DIR/$P"
  setup_greenfield "calculator" "$W"
  run_project "$P" "$W" \
    "Create a calculator in src/lib.rs with add, subtract, multiply, divide. divide returns Result on division by zero. Write 5 unit tests. Run cargo test until all pass." 7 || return 0

  P="money_split_r2"; W="$ROUND_DIR/$P"
  setup_greenfield "money-split" "$W"
  run_project "$P" "$W" \
    "Build in src/lib.rs: pub fn split_amount_cents(total: i64, people: usize) -> Result<Vec<i64>, &'static str>. Reject negative totals and zero people. Split evenly and distribute remainder to earliest entries. Write 6 unit tests. Run cargo test." 7 || return 0

  P="bounded_queue_r2"; W="$ROUND_DIR/$P"
  setup_greenfield "bounded-queue" "$W"
  run_project "$P" "$W" \
    "Build a bounded FIFO queue in src/lib.rs: pub struct BoundedQueue<T> with new(capacity), push(value), pop() -> Option<T>, peek() -> Option<&T>, len(), is_empty(). When full, push evicts oldest. Capacity 0 discards everything. Write 8 unit tests. Run cargo test." 7 || return 0

  P="slugify_r2"; W="$ROUND_DIR/$P"
  setup_greenfield "slugify" "$W"
  run_project "$P" "$W" \
    "Build a slug generator in src/lib.rs: pub fn slugify(input: &str) -> String. Lowercase, keep ASCII alphanumeric, collapse whitespace, underscores, and hyphens to a single hyphen, strip punctuation, and trim edge separators. Write 7 unit tests. Run cargo test." 7 || return 0

  finish_round
}

run_round_8() {
  start_round 8 "algorithms" "Algorithms" "Algorithms"

  P="graph_bfs"; W="$ROUND_DIR/$P"
  setup_greenfield "graph-bfs" "$W"
  run_project "$P" "$W" \
    "Build a directed graph with BFS in src/lib.rs: pub struct Graph with new(), add_edge(from: usize, to: usize), bfs(start: usize) -> Vec<usize>, has_path(from: usize, to: usize) -> bool, shortest_path(from: usize, to: usize) -> Option<Vec<usize>>. Use adjacency lists. Write 10 unit tests. Run cargo test." 8 || return 0

  P="base64_codec"; W="$ROUND_DIR/$P"
  setup_greenfield "base64-codec" "$W"
  run_project "$P" "$W" \
    "Build base64 encode and decode in src/lib.rs: pub fn encode(input: &[u8]) -> String and pub fn decode(input: &str) -> Result<Vec<u8>, String>. Implement RFC 4648 base64 with padding. Write 10 unit tests including empty input, padding cases, and invalid chars. Run cargo test." 8 || return 0

  P="interval_merge"; W="$ROUND_DIR/$P"
  setup_greenfield "interval-merge" "$W"
  run_project "$P" "$W" \
    "Build an interval merger in src/lib.rs: pub fn merge_intervals(intervals: &[(i32, i32)]) -> Vec<(i32, i32)>. Sort by start and merge overlaps. Also add pub fn insert_interval(intervals: &[(i32, i32)], new: (i32, i32)) -> Vec<(i32, i32)>. Write 10 unit tests. Run cargo test." 8 || return 0

  P="rate_limiter"; W="$ROUND_DIR/$P"
  setup_greenfield "rate-limiter" "$W"
  run_project "$P" "$W" \
    "Build a token bucket rate limiter in src/lib.rs: pub struct RateLimiter with new(capacity: u32, refill_rate: u32), try_acquire(&mut self) -> bool, available(&self) -> u32, refill(&mut self, tokens: u32). Write 8 unit tests. Run cargo test." 8 || return 0

  finish_round
}

run_round_9() {
  start_round 9 "python_expanded" "Python Expanded" "Python Expanded"

  P="python_csv"; W="$ROUND_DIR/$P"
  setup_python "$W"
  run_project "$P" "$W" \
    "Create csv_tool.py with: read_csv(filepath) -> list[dict], write_csv(filepath, rows: list[dict]), filter_rows(rows, column, value) -> list[dict], sort_rows(rows, column, reverse=False) -> list[dict]. Create test_csv_tool.py with 8 pytest tests using tempfiles. Run python3 -m pytest -v." 9 || return 0

  P="python_stack"; W="$ROUND_DIR/$P"
  setup_python "$W"
  run_project "$P" "$W" \
    "Create stack.py with a Stack class: push(val), pop() -> val raising IndexError if empty, peek() -> val, is_empty() -> bool, size() -> int, and __iter__ for iteration. Create test_stack.py with 10 pytest tests. Run python3 -m pytest -v." 9 || return 0

  P="python_linked_list"; W="$ROUND_DIR/$P"
  setup_python "$W"
  run_project "$P" "$W" \
    "Create linked_list.py with a LinkedList class: append(val), prepend(val), delete(val) -> bool, find(val) -> bool, size() -> int, to_list() -> list, reverse(). Create test_linked_list.py with 10 pytest tests. Run python3 -m pytest -v." 9 || return 0

  P="python_roman"; W="$ROUND_DIR/$P"
  setup_python "$W"
  run_project "$P" "$W" \
    "Create roman.py with to_roman(n: int) -> str and from_roman(s: str) -> int. Handle 1 to 3999. Create test_roman.py with 12 pytest tests including edge cases. Run python3 -m pytest -v." 9 || return 0

  finish_round
}

run_round_10() {
  start_round 10 "template_retry" "Template Retry" "Template Retry"

  P="viz_sparkline_r2"; W="$ROUND_DIR/$P"
  setup_template "viz_sparkline" "$W"
  run_project "$P" "$W" \
    "Read all source files in src/ and the test file in tests/. Fix all bugs so every test passes. Do NOT change test files or function signatures. Run cargo test." 10 || return 0

  P="viz_progress_bar_r2"; W="$ROUND_DIR/$P"
  setup_template "viz_progress_bar" "$W"
  run_project "$P" "$W" \
    "Read all source files in src/ and tests/. Fix all bugs so tests pass. Do NOT change test files or function signatures. Run cargo test." 10 || return 0

  P="medium_bitset_r2"; W="$ROUND_DIR/$P"
  setup_template "medium_bitset" "$W"
  run_project "$P" "$W" \
    "Read src/lib.rs and tests/bitset_tests.rs. Fix all bugs so every test passes. Do NOT change test files or public API. Run cargo test." 10 || return 0

  P="medium_json_merge_r2"; W="$ROUND_DIR/$P"
  setup_template "medium_json_merge" "$W"
  run_project "$P" "$W" \
    "Read src/lib.rs and tests/merge_tests.rs. Implement RFC 7396 JSON Merge Patch correctly. Do NOT change tests. Run cargo test." 10 || return 0

  finish_round
}

run_cycle() {
  run_round_1
  (( STOP_REQUESTED == 1 )) && return 0
  run_round_2
  (( STOP_REQUESTED == 1 )) && return 0
  run_round_3
  (( STOP_REQUESTED == 1 )) && return 0
  run_round_4
  (( STOP_REQUESTED == 1 )) && return 0
  run_round_5
  (( STOP_REQUESTED == 1 )) && return 0
  run_round_6
  (( STOP_REQUESTED == 1 )) && return 0
  run_round_7
  (( STOP_REQUESTED == 1 )) && return 0
  run_round_8
  (( STOP_REQUESTED == 1 )) && return 0
  run_round_9
  (( STOP_REQUESTED == 1 )) && return 0
  run_round_10
}

check_endpoint() {
  curl -fsS --connect-timeout 5 "$ENDPOINT/models" | grep -q "$MODEL"
}

generate_final_report() {
  local total green partial compiles wrote fail unverified tampered quarantined
  total="$(count_total_rows)"
  green="$(count_status GREEN)"
  partial="$(count_status PARTIAL)"
  unverified="$(count_status UNVERIFIED)"
  tampered="$(count_status TAMPERED_TESTS)"
  compiles="$(count_status COMPILES)"
  wrote="$(count_status WROTE)"
  fail="$(count_status FAIL)"
  quarantined="$(count_status QUARANTINED)"

  cat > "$FINAL_REPORT" << EOF
# 8-Hour System Test v6 - Final Report

- Completed: $(date)
- Results dir: $MASTER_DIR
- Configured duration: $(seconds_to_human "$DURATION_SECS")
- Actual duration: $(elapsed_human)
- Cycles started: $CURRENT_CYCLE

## Summary

| Metric | Count |
|--------|-------|
| Total projects | $total |
| GREEN | $green |
| PARTIAL | $partial |
| UNVERIFIED | $unverified |
| TAMPERED_TESTS | $tampered |
| COMPILES | $compiles |
| WROTE | $wrote |
| FAIL | $fail |
EOF
}

cat > "$MASTER_DIR/ALL_RESULTS.md" << 'EOF'
# 8-Hour System Test v4 — All Results

| Cycle | Round | Project | Status | Time(s) | Steps | SrcLines | Compiles | Tests | Reason |
|-------|-------|---------|--------|---------|-------|----------|----------|-------|--------|
EOF

echo "============================================================"
echo "  8-HOUR SYSTEM TEST v4 - $(date)"
echo "  Endpoint: $ENDPOINT"
echo "  Model: $MODEL"
echo "  Configured duration: $(seconds_to_human "$DURATION_SECS")"
echo "  Timeout: ${TIMEOUT_PER_PROJECT}s per project"
echo "  Minimum project window: ${MIN_PROJECT_WINDOW_SECS}s"
echo "  Max iterations: $MAX_ITERS per project"
echo "  Results: $MASTER_DIR"
echo "============================================================"
echo

if [[ ! -x "$SELFWARE" ]]; then
  echo "ERROR: selfware binary not found or not executable at $SELFWARE"
  exit 1
fi

if [[ ! -d "$TEMPLATES" ]]; then
  echo "ERROR: template directory not found at $TEMPLATES"
  exit 1
fi

write_state
snapshot_status
monitor_loop &
MONITOR_PID="$!"
printf '%s\n' "$MONITOR_PID" > "$MONITOR_PID_FILE"

if ! check_endpoint; then
  echo "ERROR: endpoint $ENDPOINT is not serving model $MODEL"
  exit 1
fi

append_event "Endpoint OK"

while (( $(time_remaining) >= MIN_PROJECT_WINDOW_SECS )); do
  CURRENT_CYCLE=$((CURRENT_CYCLE + 1))
  append_event "Starting cycle $CURRENT_CYCLE"
  run_cycle
  append_event "Finished cycle $CURRENT_CYCLE"
  (( STOP_REQUESTED == 1 )) && break
done

if (( STOP_REQUESTED == 0 )) && (( $(time_remaining) < MIN_PROJECT_WINDOW_SECS )); then
  append_event "Stopping because remaining time is below ${MIN_PROJECT_WINDOW_SECS}s"
fi

generate_final_report

echo
echo "============================================================"
echo "  8-HOUR SYSTEM TEST v4 COMPLETE - $(date)"
echo "  Actual duration: $(elapsed_human)"
echo "============================================================"
echo
cat "$FINAL_REPORT"
echo
echo "Results: $MASTER_DIR"
