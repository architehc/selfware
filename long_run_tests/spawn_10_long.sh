#!/bin/bash
# 10 long-running parallel selfware agents against the 122B endpoint
# Each gets a multi-phase Rust project task designed for 30+ minutes

set -e

SELFWARE="$(dirname "$0")/../target/release/selfware"
BASE="/home/ivo/selfware/long_run_tests"
ENDPOINT="https://crazyshit.ngrok.io/v1"
MODEL="txn545/Qwen3.5-122B-A10B-NVFP4"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_DIR="$BASE/longrun_${TIMESTAMP}"
mkdir -p "$RESULTS_DIR"

echo "=== Spawning 10 LONG-RUNNING agents at $(date) ==="
echo "Endpoint: $ENDPOINT"
echo "Model: $MODEL"
echo "Results: $RESULTS_DIR"
echo ""

# Each task is multi-phase: build → test → extend → refactor
TASKS=(
  "Build a complete Calculator library in src/lib.rs:
Phase 1: Create Calculator struct with add, subtract, multiply, divide (divide returns Result for zero).
Phase 2: Add power, sqrt, modulo, abs, negate methods.
Phase 3: Add a history feature — record every operation and result, with undo/redo.
Phase 4: Write 20+ unit tests covering all methods including edge cases (overflow, NaN, infinity).
Phase 5: Run cargo test and fix ALL failures until green.
Work through each phase in order. Use file_write and file_edit tools. Run cargo check after each phase."

  "Build a task management library in src/lib.rs:
Phase 1: Create Task struct (id, title, status enum, priority, created_at, tags Vec<String>).
Phase 2: Create TaskStore with add, get, list, update, delete methods using HashMap.
Phase 3: Add filter_by_status, filter_by_priority, search_by_title (case-insensitive), sort_by_priority methods.
Phase 4: Add serde Serialize/Deserialize, save_to_file and load_from_file using JSON.
Phase 5: Write 15+ tests covering CRUD, filtering, search, persistence round-trip.
Phase 6: Run cargo test and fix ALL failures.
Use file_write and file_edit tools. Run cargo check between phases."

  "Build a Rust stack-based calculator (RPN) in src/lib.rs:
Phase 1: Create RpnCalculator struct with a Vec<f64> stack. Methods: push, pop, peek.
Phase 2: Add operations: add, sub, mul, div, swap, dup, clear — each pops operands and pushes result.
Phase 3: Add evaluate(expression: &str) that tokenizes and evaluates '3 4 + 2 *' style input.
Phase 4: Add error handling: StackUnderflow, DivisionByZero, InvalidToken errors.
Phase 5: Write 20+ tests for all operations, error cases, and complex expressions.
Phase 6: Run cargo test and fix everything.
Always use file_write to create files and file_edit to modify them."

  "Build a simple in-memory database in src/lib.rs:
Phase 1: Create Table struct with column names (Vec<String>) and rows (Vec<Vec<String>>).
Phase 2: Create Database struct with HashMap<String, Table>. Methods: create_table, drop_table, insert_row.
Phase 3: Add select method that returns rows matching a where clause (column == value).
Phase 4: Add update and delete methods with where clause filtering.
Phase 5: Add order_by sorting and limit/offset pagination.
Phase 6: Write 15+ tests for all operations.
Phase 7: Run cargo test until green.
Use file_write and file_edit tools throughout."

  "Build a text statistics library in src/lib.rs:
Phase 1: Create TextStats struct. Methods: word_count, char_count, line_count, sentence_count.
Phase 2: Add word_frequency returning HashMap<String, usize> sorted by count.
Phase 3: Add average_word_length, longest_word, shortest_word, unique_word_count.
Phase 4: Add readability_score (Flesch-Kincaid or similar simple formula).
Phase 5: Add from_file that reads a text file and computes all stats.
Phase 6: Write 20+ tests with various input types (empty, single word, paragraphs, unicode).
Phase 7: Run cargo test.
Use file_write/file_edit tools. Never output code as text."

  "Build a simple event system in src/lib.rs:
Phase 1: Create EventBus struct with subscribe(event_name, callback) and emit(event_name, data).
Phase 2: Use Fn traits for callbacks. Support multiple subscribers per event.
Phase 3: Add unsubscribe, clear, list_events, subscriber_count methods.
Phase 4: Add EventLog that records all emitted events with timestamps.
Phase 5: Add once() for one-shot subscriptions that auto-remove after first trigger.
Phase 6: Write 15+ tests for subscribe, emit, unsubscribe, once, event log.
Phase 7: Run cargo test.
Always use file_write to write code to files."

  "Build a simple state machine in src/lib.rs:
Phase 1: Create StateMachine<S> with current_state, add_transition(from, event, to), trigger(event).
Phase 2: S should be Clone + Eq + Hash. Store transitions in HashMap<(S, String), S>.
Phase 3: Add on_enter and on_exit callbacks per state.
Phase 4: Add transition history tracking and can_trigger(event) -> bool.
Phase 5: Add reset(), state_count(), event_count() helpers.
Phase 6: Build a concrete example: TrafficLight with Red/Yellow/Green states.
Phase 7: Write 15+ tests including invalid transitions and callbacks.
Phase 8: Run cargo test.
Use file_write and file_edit."

  "Build a simple LRU cache in src/lib.rs:
Phase 1: Create LruCache<K, V> with capacity, get, put methods. K: Hash+Eq, V: Clone.
Phase 2: Implement using HashMap + doubly-linked list (or VecDeque for simplicity).
Phase 3: When capacity is exceeded, evict the least recently used entry.
Phase 4: Add remove, contains, len, is_empty, clear, keys, values methods.
Phase 5: Add iter() that yields entries from most to least recently used.
Phase 6: Write 20+ tests: basic get/put, eviction, capacity edge cases, iteration order.
Phase 7: Run cargo test until all pass.
Use file_write/file_edit tools exclusively."

  "Build a simple Rust matrix library in src/lib.rs:
Phase 1: Create Matrix struct storing Vec<Vec<f64>> with rows and cols.
Phase 2: Add new, zeros, identity, from_vec constructors.
Phase 3: Add add, subtract, multiply (matrix multiplication) methods.
Phase 4: Add transpose, determinant (for 2x2 and 3x3), scalar_multiply.
Phase 5: Implement Display trait for pretty printing.
Phase 6: Write 20+ tests for all operations including dimension mismatch errors.
Phase 7: Run cargo test until green.
Use file_write to create src/lib.rs. Use file_edit for modifications."

  "Build a simple command parser in src/lib.rs:
Phase 1: Create Command enum with variants: Help, Quit, Echo(String), Set(String, String), Get(String), Delete(String), List.
Phase 2: Create parse(input: &str) -> Result<Command, ParseError> function.
Phase 3: Create CommandRegistry that maps command names to handlers.
Phase 4: Add execute(command) that runs the appropriate handler.
Phase 5: Add a simple Environment (HashMap<String, String>) that Set/Get/Delete operate on.
Phase 6: Write 20+ tests for parsing, execution, error handling.
Phase 7: Run cargo test.
Always write code using file_write tool, never as text in responses."
)

for i in $(seq 0 9); do
  WORKDIR="$RESULTS_DIR/agent_$i"
  mkdir -p "$WORKDIR/src"

  cat > "$WORKDIR/Cargo.toml" << 'CARGO'
[package]
name = "agent-test"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
CARGO

  echo 'fn main() { println!("Hello"); }' > "$WORKDIR/src/main.rs"
  echo '// TODO: implement module' > "$WORKDIR/src/lib.rs"

  (cd "$WORKDIR" && git init -q && git add -A && git commit -q -m "init")

  cat > "$WORKDIR/selfware.toml" << EOF
endpoint = "$ENDPOINT"
model = "$MODEL"
max_tokens = 16384
context_length = 262144
temperature = 0.5

[safety]
allowed_paths = ["./**", "/tmp/**"]

[agent]
max_iterations = 500
step_timeout_secs = 600
native_function_calling = false
streaming = true
min_completion_steps = 20
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

  echo "  Agent $i: ${TASK%%$'\n'*}..."

  (cd "$WORKDIR" && "$SELFWARE" run --yolo "$TASK" > "$LOG" 2>&1) &
  PIDS[$i]=$!
  echo "    PID: ${PIDS[$i]} → $LOG"

  sleep 3
done

echo ""
echo "=== All 10 agents launched ==="
echo "PIDs: ${PIDS[*]}"
echo ""

# Monitor script that can be run separately
cat > "$RESULTS_DIR/monitor.sh" << 'MONITOR'
#!/bin/bash
DIR="$(dirname "$0")"
while true; do
  clear
  echo "=== $(date) ==="
  for i in $(seq 0 9); do
    f="$DIR/agent_${i}.log"
    ST=$(grep -c "Step.*Executing" "$f" 2>/dev/null || echo 0)
    AW=$(grep -c "Auto-writing\|auto-writing" "$f" 2>/dev/null || echo 0)
    DN=$(grep -c "Task complete" "$f" 2>/dev/null || echo 0)
    W="$DIR/agent_$i"
    SZ=$(wc -l "$W/src/lib.rs" 2>/dev/null | awk '{print $1}' || echo 1)
    CK=$(cd "$W" 2>/dev/null && cargo check 2>&1 | grep -c "Finished" || echo 0)
    echo "  A$i: steps=$ST writes=$AW lib=$SZ compiles=$CK done=$DN"
  done
  sleep 30
done
MONITOR
chmod +x "$RESULTS_DIR/monitor.sh"

echo "Monitor: bash $RESULTS_DIR/monitor.sh"
echo ""
echo "Quick check: for i in \$(seq 0 9); do W=$RESULTS_DIR/agent_\$i; echo \"A\$i: \$(wc -l \$W/src/lib.rs | awk '{print \$1}') lines\"; done"
echo ""

# Wait for all
echo "Waiting for all agents (this may take 30+ minutes)..."
FAILED=0
for i in $(seq 0 9); do
  wait ${PIDS[$i]} 2>/dev/null
  EXIT=$?
  LOG="$RESULTS_DIR/agent_${i}.log"
  STEPS=$(grep -c "Step.*Executing" "$LOG" 2>/dev/null || echo "0")
  AW=$(grep -c "Auto-writing" "$LOG" 2>/dev/null || echo "0")
  WORKDIR="$RESULTS_DIR/agent_$i"
  SZ=$(wc -l "$WORKDIR/src/lib.rs" 2>/dev/null | awk '{print $1}' || echo "1")

  if [ $EXIT -eq 0 ]; then STATUS="OK"; else STATUS="FAIL"; FAILED=$((FAILED+1)); fi

  # Test
  TEST_RESULT=$(cd "$WORKDIR" && cargo test 2>&1 | grep "test result:" | head -1 || echo "no tests")

  echo "  A$i: $STATUS steps=$STEPS writes=$AW lib=${SZ}L | $TEST_RESULT"
done

echo ""
echo "=== FINAL SUMMARY ==="
echo "Completed: $((10 - FAILED))/10"
echo "Results: $RESULTS_DIR"
