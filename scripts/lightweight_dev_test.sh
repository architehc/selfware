#!/bin/bash
# Lightweight Development Test - 4 instances only (1 per task)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SELFWARE_BIN="$PROJECT_DIR/target/release/selfware"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
TEST_DIR="$PROJECT_DIR/parallel_dev_tests/lightweight_$TIMESTAMP"

# Verify binary
if [[ ! -f "$SELFWARE_BIN" ]]; then
    echo "Building selfware..."
    cd "$PROJECT_DIR" && cargo build --release
fi

# Create test directories
mkdir -p "$TEST_DIR"/{flappy_bird,portfolio,tqec_sim,rust_game}

echo "=========================================="
echo "  Lightweight Development Test"
echo "=========================================="
echo "Test Directory: $TEST_DIR"
echo "Instances: 4 (1 per task)"
echo ""

# Create minimal config
cat > "$TEST_DIR/selfware.toml" << EOF
endpoint = "http://localhost:8000/v1"
model = "qwen3.5-27b"
max_tokens = 32768
temperature = 0.7

[concurrency]
max_parallel_requests = 4
max_retries = 3
timeout_secs = 180

[safety]
allowed_paths = ["$TEST_DIR/**"]
denied_paths = []

[agent]
streaming = true
max_iterations = 30
step_timeout_secs = 180
native_function_calling = true
token_budget = 500000

[continuous_work]
enabled = true
checkpoint_interval_tools = 5
auto_recovery = true
EOF

# Task prompts
declare -A TASKS
declare -A PROMPTS

TASKS[1]="flappy_bird"
PROMPTS[1]="Create a simple Flappy Bird game in HTML5 Canvas. Single index.html file with embedded CSS and JavaScript. Include: bird that falls with gravity, jumps on spacebar, pipes that scroll, collision detection, and score. Keep it under 300 lines."

TASKS[2]="portfolio"
PROMPTS[2]="Create a simple portfolio website: index.html with hero section, about section, and contact form. Use modern CSS with flexbox. Include dark mode toggle. Clean, minimalist design."

TASKS[3]="tqec_sim"
PROMPTS[3]="Create a minimal TQEC quantum simulator in Rust. Cargo.toml with necessary deps. src/main.rs that creates a simple lattice and prints it. Keep under 200 lines. Focus on clean code over features."

TASKS[4]="rust_game"
PROMPTS[4]="Create a minimal space shooter in Rust using macroquad. Cargo.toml with macroquad dependency. Single src/main.rs with player that moves, shoots bullets, and enemies spawn. Under 300 lines."

# Launch instances
for i in 1 2 3 4; do
    task="${TASKS[$i]}"
    prompt="${PROMPTS[$i]}"
    workdir="$TEST_DIR/$task"
    
    echo "[$i/4] Starting $task..."
    
    "$SELFWARE_BIN" \
        --config "$TEST_DIR/selfware.toml" \
        --mode yolo \
        --workdir "$workdir" \
        run "$prompt" > "$workdir/selfware.log" 2>&1 &
    
    echo $! > "$workdir/pid"
    echo "  Started (PID: $!)"
    sleep 2  # Small delay between launches
done

echo ""
echo "=========================================="
echo "  All 4 instances launched!"
echo "=========================================="
echo ""
echo "Monitor:"
echo "  watch -n 10 'for d in $TEST_DIR/*/; do echo \"=== \$d ===\"; ls -la \$d | tail -5; done'"
echo ""
echo "Results:"
echo "  cat $TEST_DIR/*/selfware.log | grep -E '(Created|Writing|Finished)'"
