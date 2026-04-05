#!/bin/bash
# 8-hour system test v4: Enhanced monitoring + resilience + comprehensive reporting
# Improvements over v3:
# - Real-time health monitoring with automatic recovery
# - Memory leak detection
# - Disk space monitoring
# - Structured JSON metrics output
# - Automated checkpointing
# - Better failure categorization
set -u

SELFWARE="/home/ivo/selfware/target/release/selfware"
ENDPOINT="https://crazyshit.ngrok.io/v1"
MODEL="txn545/Qwen3.5-122B-A10B-NVFP4"
TEMPLATES="/home/ivo/selfware/system_tests/projecte2e/templates"
MASTER_DIR="/home/ivo/selfware/long_run_tests/system_test_8hr_v4_$(date +%Y%m%d_%H%M%S)"
TIMEOUT_PER_PROJECT="${TIMEOUT:-1200}"
MAX_ITERS=100
START_TIME=$(date +%s)
MAX_DURATION=$((8 * 3600))
CHECKPOINT_INTERVAL=600  # Save checkpoint every 10 minutes

mkdir -p "$MASTER_DIR"

MASTER_LOG="$MASTER_DIR/master.log"
METRICS_FILE="$MASTER_DIR/metrics.jsonl"
exec > >(tee -a "$MASTER_LOG") 2>&1

echo "════════════════════════════════════════════════════════════"
echo "  8-HOUR SYSTEM TEST v4 — $(date)"
echo "  IMPROVEMENTS: Health monitoring, auto-recovery, metrics"
echo "  Endpoint: $ENDPOINT"
echo "  Model: $MODEL"
echo "  Timeout: ${TIMEOUT_PER_PROJECT}s per project"
echo "  Max iterations: $MAX_ITERS per project"
echo "  Results: $MASTER_DIR"
echo "════════════════════════════════════════════════════════════"
echo ""

# ── Helpers ──

strip_ansi() { sed -r 's/\x1B\[[0-9;]*[A-Za-z]//g'; }

elapsed_hours() {
  local now=$(date +%s)
  printf "%dh%02dm" $(( (now - START_TIME) / 3600 )) $(( ((now - START_TIME) % 3600) / 60 ))
}

time_remaining() {
  local remaining=$(( MAX_DURATION - ($(date +%s) - START_TIME) ))
  [ "$remaining" -le 0 ] && echo "0" || echo "$remaining"
}

log_metric() {
  local metric_name="$1"
  local value="$2"
  local timestamp=$(date +%s)
  echo "{\"timestamp\": $timestamp, \"metric\": \"$metric_name\", \"value\": $value}" >> "$METRICS_FILE"
}

# Health monitoring - runs in background
health_monitor() {
  while [ $(date +%s) -lt $((START_TIME + MAX_DURATION)) ]; do
    local timestamp=$(date +%s)
    local mem_usage=$(free -m | awk 'NR==2{printf "%.1f", $3*100/$2}')
    local disk_usage=$(df -h "$MASTER_DIR" | awk 'NR==2{print $5}' | tr -d '%')
    local load_avg=$(uptime | awk -F'load average:' '{print $2}' | awk '{print $1}' | tr -d ',')
    
    # Log health metrics
    echo "{\"timestamp\": $timestamp, \"memory_percent\": $mem_usage, \"disk_percent\": $disk_usage, \"load_avg\": $load_avg}" >> "$MASTER_DIR/health.jsonl"
    
    # Alert on concerning metrics
    if (( $(echo "$mem_usage > 90" | bc -l) )); then
      echo "⚠️  HIGH MEMORY USAGE: ${mem_usage}%" | tee -a "$MASTER_DIR/alerts.log"
    fi
    if [ "$disk_usage" -gt 90 ]; then
      echo "⚠️  HIGH DISK USAGE: ${disk_usage}%" | tee -a "$MASTER_DIR/alerts.log"
    fi
    
    sleep 60
  done
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

run_project() {
  local name="$1"
  local workdir="$2"
  local task="$3"
  local round="$4"
  local log="$ROUND_DIR/${name}.log"
  local attempt="${5:-1}"
  local max_attempts=2

  echo -n "  [$name] (attempt $attempt) "
  local proj_start=$(date +%s)
  
  # Record start metric
  log_metric "project_start" "{\"project\": \"$name\", \"round\": $round, \"attempt\": $attempt}"

  timeout "$TIMEOUT_PER_PROJECT" "$SELFWARE" \
    -c "$workdir/selfware.toml" \
    -C "$workdir" \
    --yolo \
    --ascii \
    --no-color \
    -p "$task" \
    > "$log" 2>&1 || true

  local proj_end=$(date +%s)
  local dur=$((proj_end - proj_start))
  local steps=$(grep -c "Step.*Executing" "$log" 2>/dev/null || echo 0)
  local label=$(grep -o "Outcome: [a-z_]*" "$log" 2>/dev/null | tail -1 || echo "none")
  local error_type=$(grep -o "ERROR: [^ ]*" "$log" 2>/dev/null | head -1 || echo "none")

  local compile="no" test_result="-" passed=0 failed_count=0 src_lines=0
  local language="unknown"

  # Rust
  if [ -f "$workdir/Cargo.toml" ]; then
    language="rust"
    for f in "$workdir/src/"*.rs; do
      [ -f "$f" ] && src_lines=$((src_lines + $(wc -l < "$f")))
    done
    local cargo_out
    cargo_out=$(cd "$workdir" && cargo check 2>&1) || true
    if echo "$cargo_out" | grep -q "Finished"; then
      compile="YES"
      local test_out
      test_out=$(cd "$workdir" && cargo test 2>&1) || true
      passed=$(echo "$test_out" | grep -o "[0-9]* passed" | awk '{s+=$1} END {print s+0}')
      failed_count=$(echo "$test_out" | grep -o "[0-9]* failed" | awk '{s+=$1} END {print s+0}')
      test_result="${passed}p/${failed_count}f"
    fi
  fi

  # Python
  local py_test_file
  py_test_file=$(find "$workdir" -maxdepth 1 -name "test_*.py" -type f 2>/dev/null | head -1)
  if [ -n "$py_test_file" ]; then
    language="python"
    src_lines=$(find "$workdir" -maxdepth 1 -name "*.py" ! -name "test_*" -exec wc -l {} + 2>/dev/null | tail -1 | awk '{print $1}') || src_lines=0
    local py_out
    py_out=$(cd "$workdir" && python3 -m pytest -v 2>&1) || true
    if echo "$py_out" | grep -q "passed"; then
      compile="YES"
      passed=$(echo "$py_out" | grep -oP "\d+ passed" | grep -o "[0-9]*") || passed=0
      failed_count=$(echo "$py_out" | grep -oP "\d+ failed" | grep -o "[0-9]*") || failed_count=0
      test_result="${passed:-0}p/${failed_count:-0}f"
    elif echo "$py_out" | grep -q "error"; then
      compile="no"
    fi
  fi

  # Go
  if [ -f "$workdir/go.mod" ]; then
    language="go"
    src_lines=$(find "$workdir" -maxdepth 1 -name "*.go" ! -name "*_test.go" -exec wc -l {} + 2>/dev/null | tail -1 | awk '{print $1}') || src_lines=0
    export XDG_RUNTIME_DIR=/tmp/xdg-run-selfware
    mkdir -p "$XDG_RUNTIME_DIR" 2>/dev/null || true
    local go_out
    go_out=$(cd "$workdir" && go test -v ./... 2>&1) || true
    if echo "$go_out" | grep -q "^ok"; then
      compile="YES"
      passed=$(echo "$go_out" | grep -c "--- PASS" || echo 0)
      failed_count=$(echo "$go_out" | grep -c "--- FAIL" || echo 0)
      test_result="${passed}p/${failed_count}f"
    elif echo "$go_out" | grep -q "FAIL"; then
      compile="YES"
      passed=$(echo "$go_out" | grep -c "--- PASS" || echo 0)
      failed_count=$(echo "$go_out" | grep -c "--- FAIL" || echo 0)
      test_result="${passed}p/${failed_count}f"
    fi
  fi

  local status="FAIL"
  if [ "$compile" = "YES" ] && [ "$passed" -gt 0 ] && [ "$failed_count" -eq 0 ]; then
    status="GREEN"
  elif [ "$compile" = "YES" ] && [ "$passed" -gt 0 ]; then
    status="PARTIAL"
  elif [ "$compile" = "YES" ]; then
    status="COMPILES"
  elif [ "$src_lines" -gt 2 ]; then
    status="WROTE"
  fi
  
  # Record detailed metric
  echo "{\"timestamp\": $(date +%s), \"project\": \"$name\", \"round\": $round, \"status\": \"$status\", \"duration\": $dur, \"steps\": $steps, \"src_lines\": $src_lines, \"compile\": \"$compile\", \"tests\": \"$test_result\", \"outcome\": \"$label\", \"language\": \"$language\", \"error_type\": \"$error_type\"}" >> "$METRICS_FILE"

  printf "%s | %ds steps=%d src=%dL comp=%s tests=%s %s\n" \
    "$status" "$dur" "$steps" "$src_lines" "$compile" "$test_result" "$label"

  printf "| %s | %s | %d | %d | %d | %s | %s | %s |\n" \
    "$name" "$status" "$dur" "$steps" "$src_lines" "$compile" "$test_result" "$label" \
    >> "$ROUND_DIR/SUMMARY.md"

  printf "| R%d | %s | %s | %d | %d | %d | %s | %s |\n" \
    "$round" "$name" "$status" "$dur" "$steps" "$src_lines" "$compile" "$test_result" \
    >> "$MASTER_DIR/ALL_RESULTS.md"
  
  # Retry logic for failed projects
  if [ "$status" = "FAIL" ] && [ "$attempt" -lt "$max_attempts" ] && [ "$src_lines" -lt 10 ]; then
    echo "    ↳ Retrying $name (insufficient code written)..."
    sleep 5
    run_project "$name" "$workdir" "$task" "$round" $((attempt + 1))
    return
  fi
  
  # Save checkpoint periodically
  if [ $(($(date +%s) % CHECKPOINT_INTERVAL)) -lt 60 ]; then
    cp "$MASTER_DIR/ALL_RESULTS.md" "$MASTER_DIR/checkpoint_$(date +%H%M).md"
  fi
}

setup_greenfield() {
  local name="$1"; local dir="$2"
  mkdir -p "$dir/src"
  cat > "$dir/Cargo.toml" << EOF
[package]
name = "$name"
version = "0.1.0"
edition = "2021"
EOF
  echo '// implement here' > "$dir/src/lib.rs"
  (cd "$dir" && git init -q && git add -A && git commit -q -m init)
  make_config "$dir"
}

setup_template() {
  local template="$1"; local dir="$2"
  cp -r "$TEMPLATES/$template" "$dir"
  rm -rf "$dir/target" "$dir/Cargo.lock" "$dir/node_modules"
  (cd "$dir" && git init -q && git add -A && git commit -q -m init)
  make_config "$dir"
}

setup_python() {
  local dir="$1"
  mkdir -p "$dir"
  make_config "$dir"
  (cd "$dir" && git init -q)
}

setup_go() {
  local dir="$1"; local module="$2"
  mkdir -p "$dir"
  cat > "$dir/go.mod" << EOF
module $module

go 1.21
EOF
  make_config "$dir"
  (cd "$dir" && git init -q && git add -A && git commit -q -m init)
}

# ── Master results header ──
cat > "$MASTER_DIR/ALL_RESULTS.md" << 'EOF'
# 8-Hour System Test v4 — All Results

| Round | Project | Status | Time(s) | Steps | SrcLines | Compiles | Tests |
|-------|---------|--------|---------|-------|----------|----------|-------|
EOF

cat > "$MASTER_DIR/METRICS_README.md" << 'EOF'
# Metrics Files

- `metrics.jsonl` - Detailed per-project metrics (JSON Lines format)
- `health.jsonl` - System health monitoring (memory, disk, load)
- `alerts.log` - Alerts for concerning conditions

## Metric Schema

```json
{
  "timestamp": 1234567890,
  "project": "project_name",
  "round": 1,
  "status": "GREEN|PARTIAL|COMPILES|WROTE|FAIL",
  "duration": 120,
  "steps": 15,
  "src_lines": 150,
  "compile": "YES|no",
  "tests": "5p/0f",
  "outcome": "completed|incomplete|...",
  "language": "rust|python|go",
  "error_type": "none|..."
}
```
EOF

# Check endpoint
if ! curl -s --connect-timeout 5 "$ENDPOINT/models" | grep -q "Qwen3.5"; then
  echo "ERROR: 122B endpoint not responding"; exit 1
fi
echo "Endpoint OK"

# Start health monitor
health_monitor &
HEALTH_PID=$!
echo "Health monitor started (PID: $HEALTH_PID)"
echo ""

# ════════════════════════════════════════════════════════════════
# ROUND 1: Greenfield fundamentals
# ════════════════════════════════════════════════════════════════
ROUND=1
ROUND_DIR="$MASTER_DIR/round_${ROUND}_greenfield"
mkdir -p "$ROUND_DIR"
cat > "$ROUND_DIR/SUMMARY.md" << 'EOF'
# Round 1: Greenfield Fundamentals

| Project | Status | Time(s) | Steps | SrcLines | Compiles | Tests | Outcome |
|---------|--------|---------|-------|----------|----------|-------|---------|
EOF

echo "╔══ ROUND $ROUND: Greenfield Fundamentals ($(elapsed_hours)) ══╗"

P="expression_eval"; W="$ROUND_DIR/$P"
setup_greenfield "expression-eval" "$W"
run_project "$P" "$W" \
  "Build an arithmetic expression evaluator in src/lib.rs using recursive descent parsing. Support: integers, +, -, *, /, parentheses, unary minus. pub fn eval(expr: &str) -> Result<f64, String>. Write 12 unit tests including: basic ops, precedence (2+3*4=14), parens ((2+3)*4=20), nested parens, unary minus, division by zero error, empty input error. Run cargo test." "$ROUND"

P="lru_cache"; W="$ROUND_DIR/$P"
setup_greenfield "lru-cache" "$W"
run_project "$P" "$W" \
  "Build an LRU cache in src/lib.rs: pub struct LruCache<K, V> with new(capacity), get(&mut self, key: &K) -> Option<&V>, put(&mut self, key: K, value: V), len(), contains_key(&K). Use HashMap + VecDeque. Evict LRU on capacity overflow. get() updates recency. Write 10 unit tests. Run cargo test." "$ROUND"

P="csv_parser"; W="$ROUND_DIR/$P"
setup_greenfield "csv-parser" "$W"
run_project "$P" "$W" \
  "Build a CSV parser in src/lib.rs: pub fn parse_csv(input: &str) -> Vec<Vec<String>>. Handle commas, quoted fields, escaped quotes, newlines inside quotes, empty fields. Also pub fn to_csv(rows: &[Vec<&str>]) -> String. Write 10 unit tests. Run cargo test." "$ROUND"

P="matrix_ops"; W="$ROUND_DIR/$P"
setup_greenfield "matrix-ops" "$W"
run_project "$P" "$W" \
  "Build a matrix library in src/lib.rs: pub struct Matrix with new(rows, cols, data: Vec<f64>), add, multiply, transpose, determinant (2x2 and 3x3), identity(n). Write 10 unit tests. Run cargo test." "$ROUND"

P="html_report"; W="$ROUND_DIR/$P"
setup_greenfield "html-report" "$W"
run_project "$P" "$W" \
  "Create an HTML report generator in src/lib.rs. ReportBuilder with new(title), add_heading(text, level), add_paragraph(text), add_table(headers, rows), add_code_block(code, lang), build() -> String (valid HTML with CSS). Write 8 unit tests. Run cargo test." "$ROUND"

echo "╚══ Round $ROUND complete ($(elapsed_hours)) ══╝"; echo ""
[ "$(time_remaining)" -le 0 ] && { echo "TIME LIMIT REACHED"; exit 0; }

# ════════════════════════════════════════════════════════════════
# ROUND 2: Template fixes (visualization focus)
# ════════════════════════════════════════════════════════════════
ROUND=2
ROUND_DIR="$MASTER_DIR/round_${ROUND}_templates"
mkdir -p "$ROUND_DIR"
cat > "$ROUND_DIR/SUMMARY.md" << 'EOF'
# Round 2: Template Fixes

| Project | Status | Time(s) | Steps | SrcLines | Compiles | Tests | Outcome |
|---------|--------|---------|-------|----------|----------|-------|---------|
EOF

echo "╔══ ROUND $ROUND: Template Fixes ($(elapsed_hours)) ══╗"

P="viz_histogram"; W="$ROUND_DIR/$P"
setup_template "viz_histogram" "$W"
run_project "$P" "$W" \
  "Read all source files in src/ and the test file in tests/. This is a histogram renderer with color support. Fix all bugs so every test passes. Do NOT change test files. Run cargo test." "$ROUND"

P="viz_ascii_table"; W="$ROUND_DIR/$P"
setup_template "viz_ascii_table" "$W"
run_project "$P" "$W" \
  "Read src/lib.rs and tests/table_tests.rs carefully. The Table struct has 3 bugs: (1) column width off-by-one in saturating_sub(1), (2) horizontal lines use '-' instead of box char '─', (3) Right alignment uses left-align format. Fix ONLY these 3 bugs. Do NOT restructure the code or change function signatures. Do NOT change tests. Run cargo test." "$ROUND"

P="viz_maze_gen"; W="$ROUND_DIR/$P"
setup_template "viz_maze_gen" "$W"
run_project "$P" "$W" \
  "Read all source files in src/ and tests/. This is a maze generator with grid, generator, and ASCII renderer. Fix all incomplete/buggy functions. Do NOT change test files. Run cargo test." "$ROUND"

P="viz_svg_chart"; W="$ROUND_DIR/$P"
setup_template "viz_svg_chart" "$W"
run_project "$P" "$W" \
  "Read all source files and tests. This is an SVG chart generator. Fix all bugs so tests pass. Do NOT change test files. Run cargo test." "$ROUND"

echo "╚══ Round $ROUND complete ($(elapsed_hours)) ══╝"; echo ""
[ "$(time_remaining)" -le 0 ] && { echo "TIME LIMIT REACHED"; exit 0; }

# ════════════════════════════════════════════════════════════════
# ROUND 3: Multi-language (Python + Go)
# ════════════════════════════════════════════════════════════════
ROUND=3
ROUND_DIR="$MASTER_DIR/round_${ROUND}_multilang"
mkdir -p "$ROUND_DIR"
cat > "$ROUND_DIR/SUMMARY.md" << 'EOF'
# Round 3: Multi-Language

| Project | Status | Time(s) | Steps | SrcLines | Compiles | Tests | Outcome |
|---------|--------|---------|-------|----------|----------|-------|---------|
EOF

echo "╔══ ROUND $ROUND: Multi-Language ($(elapsed_hours)) ══╗"

P="python_json_tool"; W="$ROUND_DIR/$P"
setup_python "$W"
run_project "$P" "$W" \
  "Create json_tool.py with: flatten_json(nested_dict, sep='.') -> dict, unflatten_json(flat_dict, sep='.') -> dict, diff_json(a, b) -> dict with 'added', 'removed', 'changed' keys. Create test_json_tool.py with 10 pytest tests. Run python3 -m pytest test_json_tool.py -v" "$ROUND"

P="python_text_stats"; W="$ROUND_DIR/$P"
setup_python "$W"
run_project "$P" "$W" \
  "Create text_stats.py with: word_count(text) -> int, char_frequency(text) -> dict, sentence_count(text) -> int, average_word_length(text) -> float, most_common_words(text, n=5) -> list[tuple]. Create test_text_stats.py with 8 pytest tests. Run python3 -m pytest -v" "$ROUND"

P="go_calculator"; W="$ROUND_DIR/$P"
setup_go "$W" "calculator"
run_project "$P" "$W" \
  "Create calculator.go with package calculator and functions: Add(a, b float64) float64, Subtract(a, b float64) float64, Multiply(a, b float64) float64, Divide(a, b float64) (float64, error). Create calculator_test.go with 8 tests. Run go test -v ./..." "$ROUND"

P="go_stack"; W="$ROUND_DIR/$P"
setup_go "$W" "stack"
run_project "$P" "$W" \
  "Create stack.go with package stack and a generic Stack[T any] type: New[T]() *Stack[T], Push(val T), Pop() (T, bool), Peek() (T, bool), Len() int, IsEmpty() bool. Create stack_test.go with 8 tests. Run go test -v ./..." "$ROUND"

echo "╚══ Round $ROUND complete ($(elapsed_hours)) ══╝"; echo ""
[ "$(time_remaining)" -le 0 ] && { echo "TIME LIMIT REACHED"; exit 0; }

# ════════════════════════════════════════════════════════════════
# ROUND 4: Hard algorithms
# ════════════════════════════════════════════════════════════════
ROUND=4
ROUND_DIR="$MASTER_DIR/round_${ROUND}_algorithms"
mkdir -p "$ROUND_DIR"
cat > "$ROUND_DIR/SUMMARY.md" << 'EOF'
# Round 4: Algorithms

| Project | Status | Time(s) | Steps | SrcLines | Compiles | Tests | Outcome |
|---------|--------|---------|-------|----------|----------|-------|---------|
EOF

echo "╔══ ROUND $ROUND: Algorithms ($(elapsed_hours)) ══╗"

P="trie"; W="$ROUND_DIR/$P"
setup_greenfield "trie" "$W"
run_project "$P" "$W" \
  "Build a trie (prefix tree) in src/lib.rs: pub struct Trie with new(), insert(word: &str), contains(word: &str) -> bool, starts_with(prefix: &str) -> bool, words_with_prefix(prefix: &str) -> Vec<String>, remove(word: &str) -> bool. Write 10 unit tests. Run cargo test." "$ROUND"

P="roman"; W="$ROUND_DIR/$P"
setup_greenfield "roman" "$W"
run_project "$P" "$W" \
  "Create a Roman numeral converter in src/lib.rs. Two functions: pub fn to_roman(n: u32) -> String (map values 1000=M, 900=CM, 500=D, 400=CD, 100=C, 90=XC, 50=L, 40=XL, 10=X, 9=IX, 5=V, 4=IV, 1=I, subtract greedily). pub fn from_roman(s: &str) -> Option<u32> (map each char to value, if current < next then subtract else add). Write 10 unit tests. Run cargo test." "$ROUND"

P="json_patch"; W="$ROUND_DIR/$P"
setup_greenfield "json-patch" "$W"
cat >> "$ROUND_DIR/$P/Cargo.toml" << 'DEPS'

[dependencies]
serde_json = "1"
DEPS
(cd "$ROUND_DIR/$P" && git add -A && git commit -q -m "add serde_json dep")
run_project "$P" "$W" \
  "Build a JSON merge patch (RFC 7396) in src/lib.rs using serde_json::Value. pub fn merge(base: &Value, patch: &Value) -> Value — recursively merge objects, null in patch removes keys, non-object patch replaces entirely. Write 10 unit tests. Run cargo test." "$ROUND"

P="ring_buffer"; W="$ROUND_DIR/$P"
setup_greenfield "ring-buffer" "$W"
run_project "$P" "$W" \
  "Build a ring buffer in src/lib.rs: pub struct RingBuffer<T> with new(capacity), push(val: T), pop() -> Option<T>, peek() -> Option<&T>, len(), is_full(), is_empty(), iter() -> impl Iterator. Use a fixed-size Vec with head/tail indices. Write 10 unit tests. Run cargo test." "$ROUND"

echo "╚══ Round $ROUND complete ($(elapsed_hours)) ══╝"; echo ""
[ "$(time_remaining)" -le 0 ] && { echo "TIME LIMIT REACHED"; exit 0; }

# ════════════════════════════════════════════════════════════════
# ROUND 5: Hard templates
# ════════════════════════════════════════════════════════════════
ROUND=5
ROUND_DIR="$MASTER_DIR/round_${ROUND}_hard_templates"
mkdir -p "$ROUND_DIR"
cat > "$ROUND_DIR/SUMMARY.md" << 'EOF'
# Round 5: Hard Templates

| Project | Status | Time(s) | Steps | SrcLines | Compiles | Tests | Outcome |
|---------|--------|---------|-------|----------|----------|-------|---------|
EOF

echo "╔══ ROUND $ROUND: Hard Templates ($(elapsed_hours)) ══╗"

P="hard_event_bus"; W="$ROUND_DIR/$P"
setup_template "hard_event_bus" "$W"
run_project "$P" "$W" \
  "Read all source files in src/ and tests/. This is a pub-sub event bus with topic filtering. Fix all bugs so every test passes. Do NOT change test files. Run cargo test." "$ROUND"

P="hard_scheduler"; W="$ROUND_DIR/$P"
setup_template "hard_scheduler" "$W"
run_project "$P" "$W" \
  "Read all source files in src/ and tests/. This is a task scheduler with priority and duration. Fix all bugs so every test passes. Do NOT change test files. Run cargo test." "$ROUND"

P="easy_calculator"; W="$ROUND_DIR/$P"
setup_template "easy_calculator" "$W"
run_project "$P" "$W" \
  "Read src/lib.rs and tests/calc_tests.rs. The calculator has bugs. Fix ONLY the bugs — do NOT rewrite or restructure. Do NOT change function signatures or the test file. Run cargo test until all tests pass." "$ROUND"

P="easy_string_ops"; W="$ROUND_DIR/$P"
setup_template "easy_string_ops" "$W"
run_project "$P" "$W" \
  "Read all files in src/ and tests/. Fix all bugs in the string operations code. Do NOT change test files or function signatures. Run cargo test until all pass." "$ROUND"

echo "╚══ Round $ROUND complete ($(elapsed_hours)) ══╝"; echo ""
[ "$(time_remaining)" -le 0 ] && { echo "TIME LIMIT REACHED"; exit 0; }

# ════════════════════════════════════════════════════════════════
# ROUND 6: Data structures stress
# ════════════════════════════════════════════════════════════════
ROUND=6
ROUND_DIR="$MASTER_DIR/round_${ROUND}_data_structures"
mkdir -p "$ROUND_DIR"
cat > "$ROUND_DIR/SUMMARY.md" << 'EOF'
# Round 6: Data Structures

| Project | Status | Time(s) | Steps | SrcLines | Compiles | Tests | Outcome |
|---------|--------|---------|-------|----------|----------|-------|---------|
EOF

echo "╔══ ROUND $ROUND: Data Structures ($(elapsed_hours)) ══╗"

P="hashmap"; W="$ROUND_DIR/$P"
setup_greenfield "hashmap" "$W"
run_project "$P" "$W" \
  "Build a simple hash map in src/lib.rs: pub struct SimpleHashMap<V> (keys are String) with new(), insert(key: &str, value: V), get(key: &str) -> Option<&V>, remove(key: &str) -> Option<V>, len(), contains_key(key: &str). Use separate chaining (Vec of Vec of (String, V)). Write 10 unit tests. Run cargo test." "$ROUND"

P="binary_search"; W="$ROUND_DIR/$P"
setup_greenfield "binary-search" "$W"
run_project "$P" "$W" \
  "Build a sorted list with binary search in src/lib.rs: pub struct SortedList<T: Ord> with new(), insert(val: T), contains(val: &T) -> bool, remove(val: &T) -> bool, len(), get(index: usize) -> Option<&T>, range(from: &T, to: &T) -> Vec<&T>. Keep internal Vec sorted. Write 10 unit tests. Run cargo test." "$ROUND"

P="state_machine"; W="$ROUND_DIR/$P"
setup_greenfield "state-machine" "$W"
run_project "$P" "$W" \
  "Build a finite state machine in src/lib.rs: pub struct StateMachine with new(initial_state: &str), add_transition(from: &str, event: &str, to: &str), handle_event(&mut self, event: &str) -> Result<&str, String>, current_state() -> &str, valid_events() -> Vec<String>. Write 8 unit tests including invalid transitions. Run cargo test." "$ROUND"

P="tokenizer"; W="$ROUND_DIR/$P"
setup_greenfield "tokenizer" "$W"
run_project "$P" "$W" \
  "Build a simple tokenizer in src/lib.rs for arithmetic expressions. pub enum Token { Number(f64), Plus, Minus, Star, Slash, LParen, RParen }. pub fn tokenize(input: &str) -> Result<Vec<Token>, String>. Handle multi-digit numbers, decimals, whitespace. Write 10 unit tests. Run cargo test." "$ROUND"

echo "╚══ Round $ROUND complete ($(elapsed_hours)) ══╝"; echo ""

# ── Final Report ──
echo ""
echo "════════════════════════════════════════════════════════════"
echo "  8-HOUR SYSTEM TEST v4 COMPLETE — $(date)"
echo "  Total duration: $(elapsed_hours)"
echo "════════════════════════════════════════════════════════════"
echo ""
cat "$MASTER_DIR/ALL_RESULTS.md"
echo ""

# Calculate statistics
TOTAL=$(grep "^| R" "$MASTER_DIR/ALL_RESULTS.md" 2>/dev/null | wc -l)
GREEN=$(grep -c "| GREEN |" "$MASTER_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
PARTIAL=$(grep -c "| PARTIAL |" "$MASTER_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
COMPILES=$(grep -c "| COMPILES |" "$MASTER_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
WROTE=$(grep -c "| WROTE |" "$MASTER_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)
FAIL=$(grep -c "| FAIL |" "$MASTER_DIR/ALL_RESULTS.md" 2>/dev/null || echo 0)

# Language breakdown
RUST_COUNT=$(grep '"language": "rust"' "$METRICS_FILE" 2>/dev/null | wc -l)
PYTHON_COUNT=$(grep '"language": "python"' "$METRICS_FILE" 2>/dev/null | wc -l)
GO_COUNT=$(grep '"language": "go"' "$METRICS_FILE" 2>/dev/null | wc -l)

# Generate final report
cat > "$MASTER_DIR/FINAL_REPORT.md" << EOF
# 8-Hour System Test v4 - Final Report

**Completed**: $(date)  
**Duration**: $(elapsed_hours)  
**Results Directory**: $MASTER_DIR

## Summary Statistics

| Metric | Count | Percentage |
|--------|-------|------------|
| **Total Projects** | $TOTAL | 100% |
| 🟢 GREEN (all pass) | $GREEN | $(awk "BEGIN {printf \"%.1f\", ($GREEN/$TOTAL)*100}")% |
| 🟡 PARTIAL (some fail) | $PARTIAL | $(awk "BEGIN {printf \"%.1f\", ($PARTIAL/$TOTAL)*100}")% |
| 🔵 COMPILES (no tests) | $COMPILES | $(awk "BEGIN {printf \"%.1f\", ($COMPILES/$TOTAL)*100}")% |
| ⚪ WROTE (no compile) | $WROTE | $(awk "BEGIN {printf \"%.1f\", ($WROTE/$TOTAL)*100}")% |
| 🔴 FAIL | $FAIL | $(awk "BEGIN {printf \"%.1f\", ($FAIL/$TOTAL)*100}")% |

## Language Breakdown

| Language | Projects |
|----------|----------|
| Rust | $RUST_COUNT |
| Python | $PYTHON_COUNT |
| Go | $GO_COUNT |

## Success Criteria

- **Target**: ≥70% GREEN or PARTIAL
- **Minimum**: ≤10% FAIL

## Result

$(if [ "$GREEN" -ge $((TOTAL * 70 / 100)) ]; then echo "✅ **PASSED** - Target achieved"; elif [ "$FAIL" -le $((TOTAL * 10 / 100)) ]; then echo "⚠️ **PARTIAL** - Minimum achieved"; else echo "❌ **FAILED** - Below minimum threshold"; fi)

## Files

- Full results: \`ALL_RESULTS.md\`
- Detailed metrics: \`metrics.jsonl\`
- Health monitoring: \`health.jsonl\`
- Alerts: \`alerts.log\`
- Master log: \`master.log\`
EOF

echo "📊 FINAL STATISTICS:"
echo "  Total: $TOTAL | 🟢 $GREEN | 🟡 $PARTIAL | 🔵 $COMPILES | ⚪ $WROTE | 🔴 $FAIL"
echo "  Rust: $RUST_COUNT | Python: $PYTHON_COUNT | Go: $GO_COUNT"
echo ""
echo "Results: $MASTER_DIR"
echo "Report: $MASTER_DIR/FINAL_REPORT.md"

# Stop health monitor
kill $HEALTH_PID 2>/dev/null || true
