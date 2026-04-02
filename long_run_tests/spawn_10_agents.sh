#!/bin/bash
# Spawn 10 parallel selfware agents against the 122B endpoint
# Each gets a different Rust project task

set -e

SELFWARE="$(dirname "$0")/../target/release/selfware"
BASE="/home/ivo/selfware/long_run_tests"
ENDPOINT="https://crazyshit.ngrok.io/v1"
MODEL="txn545/Qwen3.5-122B-A10B-NVFP4"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_DIR="$BASE/results_${TIMESTAMP}"
mkdir -p "$RESULTS_DIR"

echo "=== Spawning 10 parallel agents at $(date) ==="
echo "Endpoint: $ENDPOINT"
echo "Model: $MODEL"
echo "Results: $RESULTS_DIR"
echo ""

# Array of tasks — each is a self-contained Rust project task
TASKS=(
  "Create a new Rust file src/lib.rs with a Calculator struct that has methods add, subtract, multiply, divide (returning Result for divide-by-zero). Include comprehensive unit tests. Run cargo test to verify."
  "Create a Rust CLI todo app in src/main.rs using clap. Support: add <task>, list, done <id>, remove <id>. Store tasks in a JSON file. Include error handling. Run cargo check."
  "Write a Rust module src/lib.rs implementing a simple key-value store using HashMap with get, set, delete, list_keys methods. Add serde serialization to save/load from a JSON file. Write 10+ unit tests."
  "Implement a Rust markdown-to-HTML converter in src/lib.rs. Support: headers (#), bold (**), italic (*), code blocks, links. Write unit tests for each feature. Run cargo test."
  "Create a Rust HTTP client wrapper in src/lib.rs that provides get() and post() methods using reqwest. Add retry logic with exponential backoff. Include timeout configuration. Write mock tests."
  "Build a simple Rust task scheduler in src/lib.rs. Tasks have a name, priority (1-5), and due date. Support add, list sorted by priority, mark complete, overdue check. Write tests."
  "Implement a Rust config file parser in src/lib.rs that reads TOML files into typed structs. Support nested sections, arrays, and environment variable overrides. Write tests with temp files."
  "Create a Rust string utilities module in src/lib.rs with functions: slug_from_title, truncate_with_ellipsis, word_count, extract_emails, remove_html_tags. Write property-based tests."
  "Write a Rust file watcher in src/lib.rs that detects changes in a directory. Track file creation, modification, deletion. Emit events. Include tests using tempdir."
  "Implement a simple Rust expression evaluator in src/lib.rs. Parse and evaluate: numbers, +, -, *, /, parentheses. Use recursive descent parsing. Write tests for edge cases."
)

for i in $(seq 0 9); do
  WORKDIR="$RESULTS_DIR/agent_$i"
  mkdir -p "$WORKDIR/src"

  # Create minimal Cargo.toml
  cat > "$WORKDIR/Cargo.toml" << 'CARGO'
[package]
name = "agent-test"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
CARGO

  # Create minimal source files
  echo 'fn main() { println!("Hello"); }' > "$WORKDIR/src/main.rs"
  echo '// TODO: implement module' > "$WORKDIR/src/lib.rs"

  # Init git
  (cd "$WORKDIR" && git init -q && git add -A && git commit -q -m "init")

  # Create selfware config
  cat > "$WORKDIR/selfware.toml" << EOF
endpoint = "$ENDPOINT"
model = "$MODEL"
max_tokens = 16384
context_length = 262144
temperature = 0.6

[safety]
allowed_paths = ["./**", "/tmp/**"]

[agent]
max_iterations = 200
step_timeout_secs = 600
native_function_calling = false
streaming = true
min_completion_steps = 15
require_verification_before_completion = true

[continuous_work]
enabled = true
checkpoint_interval_tools = 5
checkpoint_interval_secs = 120
auto_recovery = true
max_recovery_attempts = 10

[extra_body]
chat_template_kwargs = { enable_thinking = false }

[retry]
max_retries = 10
base_delay_ms = 2000
max_delay_ms = 120000
EOF

  TASK="${TASKS[$i]}"
  LOG="$RESULTS_DIR/agent_${i}.log"

  echo "  Agent $i: ${TASK:0:60}..."

  (cd "$WORKDIR" && "$SELFWARE" run --yolo "$TASK" > "$LOG" 2>&1) &
  PIDS[$i]=$!
  echo "    PID: ${PIDS[$i]} → $LOG"

  # Stagger launches by 2s to avoid API rate limits
  sleep 2
done

echo ""
echo "=== All 10 agents launched ==="
echo "PIDs: ${PIDS[*]}"
echo ""
echo "Monitor with:"
echo "  watch -n 5 'for f in $RESULTS_DIR/agent_*.log; do echo \"=== \$(basename \$f) ===\"; tail -3 \$f; echo; done'"
echo ""
echo "Check results with:"
echo "  for d in $RESULTS_DIR/agent_*/; do echo \"=== \$(basename \$d) ===\"; cd \$d && git diff --stat && cargo test 2>&1 | tail -3; cd -; done"
echo ""

# Wait for all agents
echo "Waiting for all agents to complete..."
FAILED=0
for i in $(seq 0 9); do
  wait ${PIDS[$i]} 2>/dev/null
  EXIT=$?
  LOG="$RESULTS_DIR/agent_${i}.log"
  STEPS=$(grep -c "Step.*Executing" "$LOG" 2>/dev/null || echo "0")
  EDITS=$(grep -c "file_edit\|file_write" "$LOG" 2>/dev/null || echo "0")
  DURATION=$(tail -1 "$LOG" 2>/dev/null | grep -o '[0-9]*[sm]' | head -1 || echo "?")

  if [ $EXIT -eq 0 ]; then
    STATUS="OK"
  else
    STATUS="FAIL(exit=$EXIT)"
    FAILED=$((FAILED + 1))
  fi

  # Check if any source files were actually modified
  WORKDIR="$RESULTS_DIR/agent_$i"
  CHANGED=$(cd "$WORKDIR" && git diff --stat -- src/ | wc -l)

  echo "  Agent $i: $STATUS | steps=$STEPS edits=$EDITS files_changed=$CHANGED duration=$DURATION"
done

echo ""
echo "=== SUMMARY ==="
echo "Completed: $((10 - FAILED))/10 succeeded, $FAILED failed"
echo "Results in: $RESULTS_DIR"
