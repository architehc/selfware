#!/bin/bash
# Advanced Development Test Runner with Visual Feedback Loop
# Tests selfware on real end-to-end projects with screenshot reviews

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SELFWARE_BIN="$PROJECT_DIR/target/release/selfware"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
TEST_BASE="$PROJECT_DIR/parallel_dev_tests"
TEST_DIR="$TEST_BASE/run_$TIMESTAMP"

# Configuration
MAX_ITERATIONS=50
STEP_TIMEOUT=300
SCREENSHOT_INTERVAL=60  # Take screenshots every 60 seconds

# Check endpoint availability
check_endpoint() {
    local endpoint=${1:-"http://localhost:8000"}
    if curl -s "$endpoint/v1/models" > /dev/null 2>&1; then
        echo "✅ Endpoint available: $endpoint"
        return 0
    else
        echo "❌ Endpoint unavailable: $endpoint"
        return 1
    fi
}

# Create test structure
create_test_env() {
    echo "Creating test environment..."
    mkdir -p "$TEST_DIR"/{flappy_bird,portfolio_website,tqec_sim,rust_game,claude_review}
    mkdir -p "$TEST_DIR"/{screenshots,logs,artifacts}
    
    for task in flappy_bird portfolio_website tqec_sim rust_game; do
        for i in 1 2 3 4; do
            mkdir -p "$TEST_DIR/$task/instance_$i"
            mkdir -p "$TEST_DIR/screenshots/$task/instance_$i"
        done
    done
    
    echo "Test directory: $TEST_DIR"
}

# Create specialized config for development tests
create_dev_config() {
    local config_file="$TEST_DIR/selfware-dev.toml"
    
    cat > "$config_file" << EOF
# Development test configuration
endpoint = "http://localhost:8000/v1"
model = "qwen3.5-27b"
max_tokens = 81920
temperature = 0.7

[extra_body]
top_p = 0.95
top_k = 20
min_p = 0.0

context_length = 1048576

[concurrency]
max_parallel_requests = 16
max_retries = 5
timeout_secs = 300

[safety]
allowed_paths = ["$TEST_DIR/**", "./**", "~/**"]
denied_paths = ["**/.env", "**/secrets/**", "**/.ssh/**"]

[agent]
streaming = true
max_iterations = $MAX_ITERATIONS
step_timeout_secs = $STEP_TIMEOUT
native_function_calling = true
token_budget = 900000
token_safety_margin = 100000

[continuous_work]
enabled = true
checkpoint_interval_tools = 10
checkpoint_interval_secs = 300
auto_recovery = true
max_recovery_attempts = 5

[retry]
max_retries = 10
base_delay_ms = 2000
max_delay_ms = 120000
EOF

    echo "$config_file"
}

# Task definitions with detailed prompts
declare -A TASK_NAMES
declare -A TASK_PROMPTS

TASK_NAMES["flappy_bird"]='Flappy Bird Game'
TASK_PROMPTS["flappy_bird"]='Create a complete, playable Flappy Bird game in a single HTML file.

REQUIREMENTS:
1. HTML5 Canvas-based game with smooth 60fps animation
2. Game physics: gravity, jump on space/click, pipe collision
3. Procedurally generated pipes with gaps
4. Score tracking (localStorage for high score)
5. Game states: menu, playing, game over with restart
6. Visual polish: bird animation, background parallax, particle effects on death
7. Sound effects (optional but preferred)
8. Mobile touch support

DELIVERABLES:
- index.html (self-contained with embedded CSS/JS)
- Must be playable by opening in browser

ITERATIVE APPROACH:
1. First create basic working game
2. Then enhance visuals and polish
3. Add advanced features last

Quality target: This should look like a professional mini-game.'

TASK_NAMES["portfolio_website"]='Portfolio Website'
TASK_PROMPTS["portfolio_website"]='Create a stunning, modern portfolio website for a software developer.

REQUIREMENTS:
1. Single-page design with smooth scroll navigation
2. Sections: Hero (with animated intro), About, Projects (3+ with cards), Skills, Contact
3. Modern CSS: CSS Grid, Flexbox, CSS variables for theming
4. Dark/Light mode toggle with persistence
5. Responsive: Mobile-first, works on all devices
6. Animations: Fade-in on scroll, hover effects, loading animations
7. Interactive elements: Project filters, skill bars, contact form
8. Professional typography (Google Fonts)

DELIVERABLES:
- index.html (semantic HTML5)
- styles.css (modern CSS with variables)
- script.js (vanilla JS, no frameworks)
- README.md with setup instructions

DESIGN INSPIRATION:
- Clean, minimalist aesthetic
- Accent color: #6366f1 (indigo)
- Smooth transitions (300-500ms)

Quality target: Portfolio-ready, could be deployed to GitHub Pages.'

TASK_NAMES["tqec_sim"]='TQEC Quantum Simulator'
TASK_PROMPTS["tqec_sim"]='Create a TQEC (Topological Quantum Error Correction) simulator in Rust.

BACKGROUND:
TQEC uses surface codes to protect quantum information. The simulator models a 2D lattice of qubits with stabilizer measurements.

REQUIREMENTS:
1. Surface Code Lattice:
   - 2D grid of data qubits and ancilla qubits
   - Configurable lattice size (e.g., 5x5 to 21x21)
   
2. Stabilizer Operations:
   - X-stabilizers (star operators) measure parity
   - Z-stabilizers (plaquette operators) measure parity
   - Syndrome extraction after each round
   
3. Error Models:
   - Bit-flip errors (X errors)
   - Phase-flip errors (Z errors)
   - Depolarizing noise (probabilistic)
   - Configurable error rates
   
4. Decoder:
   - Minimum Weight Perfect Matching (MWPM) decoder
   - Or simpler: Union-Find decoder
   - Correct errors based on syndrome

5. Output:
   - CLI to run simulations
   - ASCII visualization of lattice state
   - Statistics: logical error rate vs physical error rate
   - Export to SVG for visualization

DELIVERABLES:
- Cargo.toml with dependencies
- src/lattice.rs (grid structure)
- src/stabilizer.rs (measurement ops)
- src/errors.rs (error models)
- src/decoder.rs (MWPM or Union-Find)
- src/visualizer.rs (ASCII/SVG output)
- src/main.rs (CLI)
- examples/basic.rs (usage example)
- tests/ directory with unit tests

CRATE STRUCTURE:
Use workspace structure if needed. Focus on clean API and comprehensive tests.

Quality target: Production-ready Rust crate that could be published to crates.io.'

TASK_NAMES["rust_game"]='Space Shooter Game'
TASK_PROMPTS["rust_game"]='Create a Space Shooter game in Rust using macroquad.

REQUIREMENTS:
1. Core Gameplay:
   - Player ship at bottom, move with WASD/Arrow keys
   - Auto-fire or space to shoot
   - Enemies spawn from top in waves
   - Collision detection (AABB or pixel-perfect)
   - Score system and lives (3 hearts)
   - Game Over with restart
   
2. Entities:
   - Player: spaceship sprite, thruster particles
   - Enemies: 2-3 different types (basic, fast, tank)
   - Bullets: player shots, enemy shots
   - Power-ups: shield, rapid fire, spread shot
   - Particles: explosions, engine trails
   
3. Visual Polish:
   - Starfield background with parallax
   - Screen shake on damage
   - Flash effects on hit
   - UI: score, lives, wave number
   
4. Audio:
   - Shoot sound, explosion sound
   - Background music (optional)

DELIVERABLES:
- Cargo.toml with macroquad = "0.4"
- src/main.rs (game loop)
- src/player.rs (player logic)
- src/enemy.rs (enemy AI and spawning)
- src/bullet.rs (projectiles)
- src/collision.rs (hit detection)
- src/particle.rs (particle system)
- src/ui.rs (score/lives display)
- assets/ folder with sprites (or generate procedurally)

COMPILATION:
Must compile with "cargo run" and run at 60fps.

Quality target: Polished mini-game worthy of itch.io.'

# Launch a single instance
launch_instance() {
    local task=$1
    local instance=$2
    local workdir="$TEST_DIR/$task/instance_$instance"
    local config=$3
    local prompt="${TASK_PROMPTS[$task]}"
    
    echo "  [$task instance_$instance] Launching..."
    
    # Create a focused prompt for this instance
    local instance_prompt="$prompt

INSTANCE FOCUS: This is instance $instance of 4. Try a unique approach or variation."
    
    # Run selfware
    "$SELFWARE_BIN" \
        --config "$config" \
        --mode yolo \
        --workdir "$workdir" \
        run "$instance_prompt" > "$workdir/selfware.log" 2>&1 &
    
    local pid=$!
    echo $pid > "$workdir/pid"
    echo "    Started (PID: $pid)"
}

# Screenshot capture daemon (runs in background)
screenshot_daemon() {
    local test_dir=$1
    
    # Install playwright if available
    if command -v npm &> /dev/null; then
        npm list -g @playwright/test &> /dev/null || npm install -g @playwright/test 2>/dev/null || true
    fi
    
    while true; do
        sleep $SCREENSHOT_INTERVAL
        
        for task in flappy_bird portfolio_website; do
            for i in 1 2 3 4; do
                local workdir="$test_dir/$task/instance_$i"
                local screenshot_dir="$test_dir/screenshots/$task/instance_$i"
                
                if [[ -f "$workdir/index.html" ]]; then
                    mkdir -p "$screenshot_dir"
                    local timestamp=$(date +%H%M%S)
                    
                    # Try to screenshot with playwright
                    if command -v npx &> /dev/null; then
                        npx playwright screenshot \
                            --browser=chromium \
                            --viewport-size=1280,720 \
                            "file://$workdir/index.html" \
                            "$screenshot_dir/screenshot_$timestamp.png" 2>/dev/null || true
                    fi
                fi
            done
        done
    done
}

# Main execution
main() {
    echo "=========================================="
    echo "  Advanced Development Test Runner"
    echo "=========================================="
    echo ""
    
    # Check endpoint
    if ! check_endpoint; then
        echo ""
        echo "⚠️  WARNING: LLM endpoint not available!"
        echo "Please start your vLLM/sglang server:"
        echo "  Example: vllm serve Qwen/Qwen3.5-27B-FP8 --tensor-parallel-size 1"
        echo ""
        read -p "Continue anyway? (y/n) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi
    
    # Create environment
    create_test_env
    CONFIG=$(create_dev_config)
    
    echo ""
    echo "Configuration: $CONFIG"
    echo ""
    
    # Launch all instances
    echo "Launching 16 parallel development instances..."
    echo ""
    
    for task in flappy_bird portfolio_website tqec_sim rust_game; do
        echo "Task: ${TASK_NAMES[$task]}"
        for i in 1 2 3 4; do
            launch_instance "$task" "$i" "$CONFIG"
            sleep 0.5  # Small delay to prevent thundering herd
        done
        echo ""
    done
    
    # Start screenshot daemon
    echo "Starting screenshot capture daemon..."
    screenshot_daemon "$TEST_DIR" &
    echo $! > "$TEST_DIR/screenshot_daemon.pid"
    
    echo ""
    echo "=========================================="
    echo "  All instances launched!"
    echo "=========================================="
    echo ""
    echo "Test Directory: $TEST_DIR"
    echo ""
    echo "Monitoring commands:"
    echo "  $TEST_DIR/monitor.sh        - Check all instances"
    echo "  $TEST_DIR/summary.sh        - Generate results report"
    echo "  $TEST_DIR/stop.sh           - Stop all instances"
    echo "  $TEST_DIR/visual_review.sh  - Review screenshots"
    echo ""
}

# Create helper scripts
create_helper_scripts() {
    local test_dir=$1
    
    # Monitor script
    cat > "$test_dir/monitor.sh" << 'EOF'
#!/bin/bash
TEST_DIR="$(dirname "$0")"

echo "=========================================="
echo "  Development Test Monitor"
echo "=========================================="
echo "Time: $(date)"
echo ""

for task in flappy_bird portfolio_website tqec_sim rust_game; do
    echo "=== $task ==="
    for i in 1 2 3 4; do
        workdir="$TEST_DIR/$task/instance_$i"
        pid_file="$workdir/pid"
        
        # Check status
        if [[ -f "$pid_file" ]]; then
            pid=$(cat "$pid_file" 2>/dev/null)
            if kill -0 "$pid" 2>/dev/null; then
                status="🟢 RUNNING"
            else
                status="✅ FINISHED"
            fi
        else
            status="⚪ NOT FOUND"
        fi
        
        # Count deliverables
        html_count=$(find "$workdir" -name "*.html" 2>/dev/null | wc -l)
        rs_count=$(find "$workdir" -name "*.rs" 2>/dev/null | wc -l)
        cargo_count=$(find "$workdir" -name "Cargo.toml" 2>/dev/null | wc -l)
        
        printf "  instance_%d: %-12s HTML:%d RS:%d Cargo:%d\n" "$i" "$status" "$html_count" "$rs_count" "$cargo_count"
    done
    echo ""
done

echo "=========================================="
echo "Screenshots: $TEST_DIR/screenshots/"
echo "Logs: $TEST_DIR/*/instance_*/selfware.log"
EOF
    chmod +x "$test_dir/monitor.sh"
    
    # Summary script
    cat > "$test_dir/summary.sh" << 'EOF'
#!/bin/bash
TEST_DIR="$(dirname "$0")"
REPORT="$TEST_DIR/RESULTS.md"

echo "# Selfware Development Test Results" > "$REPORT"
echo "" >> "$REPORT"
echo "**Generated:** $(date)" >> "$REPORT"
echo "" >> "$REPORT"

for task in flappy_bird portfolio_website tqec_sim rust_game; do
    echo "## $task" >> "$REPORT"
    echo "" >> "$REPORT"
    
    for i in 1 2 3 4; do
        workdir="$TEST_DIR/$task/instance_$i"
        echo "### Instance $i" >> "$REPORT"
        echo "" >> "$REPORT"
        
        # Files created
        echo "**Files Created:**" >> "$REPORT"
        echo '```' >> "$REPORT"
        find "$workdir" -type f \( -name "*.html" -o -name "*.css" -o -name "*.js" -o -name "*.rs" -o -name "*.toml" -o -name "*.md" \) 2>/dev/null | while read f; do
            size=$(du -h "$f" 2>/dev/null | cut -f1)
            rel_path=${f#$workdir/}
            echo "$size  $rel_path"
        done >> "$REPORT"
        echo '```' >> "$REPORT"
        echo "" >> "$REPORT"
        
        # Screenshots
        screenshot_dir="$TEST_DIR/screenshots/$task/instance_$i"
        if [[ -d "$screenshot_dir" ]]; then
            count=$(find "$screenshot_dir" -name "*.png" 2>/dev/null | wc -l)
            echo "**Screenshots:** $count captured" >> "$REPORT"
            echo "" >> "$REPORT"
        fi
        
        # Last log entries
        if [[ -f "$workdir/selfware.log" ]]; then
            echo "**Last Activity:**" >> "$REPORT"
            echo '```' >> "$REPORT"
            tail -30 "$workdir/selfware.log" 2>/dev/null >> "$REPORT"
            echo '```' >> "$REPORT"
            echo "" >> "$REPORT"
        fi
    done
done

echo "Report generated: $REPORT"
cat "$REPORT"
EOF
    chmod +x "$test_dir/summary.sh"
    
    # Stop script
    cat > "$test_dir/stop.sh" << 'EOF'
#!/bin/bash
TEST_DIR="$(dirname "$0")"
echo "Stopping all instances..."

# Stop selfware instances
for task in flappy_bird portfolio_website tqec_sim rust_game; do
    for i in 1 2 3 4; do
        pid_file="$TEST_DIR/$task/instance_$i/pid"
        if [[ -f "$pid_file" ]]; then
            pid=$(cat "$pid_file" 2>/dev/null)
            if kill -0 "$pid" 2>/dev/null; then
                echo "  Stopping $task instance $i (PID: $pid)"
                kill "$pid" 2>/dev/null || true
            fi
        fi
    done
done

# Stop screenshot daemon
if [[ -f "$TEST_DIR/screenshot_daemon.pid" ]]; then
    pid=$(cat "$TEST_DIR/screenshot_daemon.pid")
    if kill -0 "$pid" 2>/dev/null; then
        echo "  Stopping screenshot daemon (PID: $pid)"
        kill "$pid" 2>/dev/null || true
    fi
fi

echo "All stopped."
EOF
    chmod +x "$test_dir/stop.sh"
    
    # Visual review script
    cat > "$test_dir/visual_review.sh" << 'EOF'
#!/bin/bash
TEST_DIR="$(dirname "$0")"

echo "=========================================="
echo "  Visual Review - Screenshots"
echo "=========================================="
echo ""

for task in flappy_bird portfolio_website; do
    echo "=== $task ==="
    for i in 1 2 3 4; do
        screenshot_dir="$TEST_DIR/screenshots/$task/instance_$i"
        if [[ -d "$screenshot_dir" ]]; then
            count=$(find "$screenshot_dir" -name "*.png" 2>/dev/null | wc -l)
            latest=$(ls -t "$screenshot_dir"/*.png 2>/dev/null | head -1)
            echo "  instance_$i: $count screenshots"
            if [[ -n "$latest" ]]; then
                echo "    Latest: $(basename $latest)"
            fi
        fi
    done
    echo ""
done

echo "To view screenshots, open: $TEST_DIR/screenshots/"
EOF
    chmod +x "$test_dir/visual_review.sh"
}

# Run main
main

# Create helper scripts after TEST_DIR is set
create_helper_scripts "$TEST_DIR"

echo "=========================================="
echo "  Setup complete!"
echo "=========================================="
